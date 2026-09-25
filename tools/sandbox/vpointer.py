#!/usr/bin/env python3
"""A virtual pointer that stays: one zwlr_virtual_pointer_v1 on the Wayland
display named by $WAYLAND_DISPLAY (under $XDG_RUNTIME_DIR), driven by lines
read from a FIFO.

    vpointer.py FIFO WIDTH HEIGHT

    to X Y          absolute motion to X,Y (output pixels)
    click [B]       press and release B: left (default), right, middle
    scroll DY       vertical wheel, DY steps (negative is up)

A pointer created and destroyed per command (wlrctl) flips the seat's
pointer capability each time, and a client binding its wl_pointer on that
flip misses the click that follows. This one is created once, so the seat
keeps a pointer for the whole run.
"""

import os
import socket
import struct
import sys
import time

BUTTONS = {"left": 0x110, "right": 0x111, "middle": 0x112}


def string(text):
    data = text.encode() + b"\0"
    return struct.pack("<I", len(data)) + data + b"\0" * (-len(data) % 4)


class Display:
    def __init__(self, path):
        self.sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.sock.connect(path)
        self.next_id = 2
        self.buf = b""

    def new_id(self):
        self.next_id += 1
        return self.next_id - 1

    def send(self, obj, opcode, payload=b""):
        size = 8 + len(payload)
        self.sock.sendall(struct.pack("<II", obj, (size << 16) | opcode) + payload)

    def events(self):
        while True:
            while len(self.buf) >= 8:
                obj, word = struct.unpack_from("<II", self.buf)
                size = word >> 16
                if len(self.buf) < size:
                    break
                body = self.buf[8:size]
                self.buf = self.buf[size:]
                yield obj, word & 0xFFFF, body
            chunk = self.sock.recv(65536)
            if not chunk:
                return
            self.buf += chunk


def main():
    fifo, width, height = sys.argv[1], int(sys.argv[2]), int(sys.argv[3])
    d = Display(os.path.join(os.environ["XDG_RUNTIME_DIR"], os.environ["WAYLAND_DISPLAY"]))
    registry, done = d.new_id(), d.new_id()
    d.send(1, 1, struct.pack("<I", registry))          # wl_display.get_registry
    d.send(1, 0, struct.pack("<I", done))              # wl_display.sync
    manager_name = None
    for obj, op, body in d.events():
        if obj == registry and op == 0:                # wl_registry.global
            (name,) = struct.unpack_from("<I", body)
            (length,) = struct.unpack_from("<I", body, 4)
            iface = body[8:8 + length - 1].decode()
            if iface == "zwlr_virtual_pointer_manager_v1":
                manager_name = name
        elif obj == 1 and op == 0:                     # wl_display.error
            sys.exit("wayland error: %r" % body)
        elif obj == done:
            break
    if manager_name is None:
        sys.exit("no zwlr_virtual_pointer_manager_v1")
    manager, pointer = d.new_id(), d.new_id()
    d.send(registry, 0, struct.pack("<I", manager_name) + string("zwlr_virtual_pointer_manager_v1")
           + struct.pack("<II", 1, manager))
    d.send(manager, 0, struct.pack("<II", 0, pointer))  # create_virtual_pointer(seat=null)

    def now():
        return int(time.monotonic() * 1000) & 0xFFFFFFFF

    def frame():
        d.send(pointer, 4)

    while True:
        with open(fifo) as commands:
            for line in commands:
                words = line.split()
                if not words:
                    continue
                if words[0] == "to":
                    x, y = int(words[1]), int(words[2])
                    d.send(pointer, 1, struct.pack("<IIIII", now(), x, y, width, height))
                    frame()
                elif words[0] == "click":
                    button = BUTTONS[words[1] if len(words) > 1 else "left"]
                    d.send(pointer, 2, struct.pack("<III", now(), button, 1))
                    frame()
                    time.sleep(0.05)
                    d.send(pointer, 2, struct.pack("<III", now(), button, 0))
                    frame()
                elif words[0] == "scroll":
                    steps = int(words[1])
                    d.send(pointer, 5, struct.pack("<I", 0))                   # axis_source wheel
                    d.send(pointer, 7, struct.pack("<IIii", now(), 0, steps * 15 * 256, steps))
                    frame()


if __name__ == "__main__":
    main()
