#!/usr/bin/python3
"""Isolated GCR client -> bridge -> morf -> encrypted response integration."""
import os,subprocess,time
from pathlib import Path
import gi
gi.require_version('Gcr','4')
from gi.repository import Gcr,Gio,GLib
root=Path(__file__).resolve().parents[2]
env=dict(os.environ,MORF_KEYRING_HELPER=str(root/'target/dist/morf-keyring'),KEYRING_TEST_BUS=os.environ['DBUS_SESSION_BUS_ADDRESS'])
env.pop('LD_LIBRARY_PATH',None);env.pop('XDG_DATA_DIRS',None)
child=subprocess.Popen([str(root/'target/release/morf'),'test','--no-dbus',str(root/'tools/keyring/ui_spec.lua')],env=env,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
context=GLib.MainContext.default();bus=Gio.bus_get_sync(Gio.BusType.SESSION,None)
result=[];prompt=None;started=False;deadline=time.monotonic()+25

def answered(obj,response,*args):
    try:result.append(obj.password_finish(response))
    except GLib.Error:result.append(None)
def opened(_obj,response,*args):
    global prompt
    prompt=Gcr.SystemPrompt.open_finish(response)
    prompt.set_title('Disposable integration test');prompt.set_message('Unlock a test prompt')
    prompt.password_async(None,answered,None)

while time.monotonic()<deadline and child.poll() is None:
    while context.pending():context.iteration(False)
    if not started:
        owned=bus.call_sync('org.freedesktop.DBus','/org/freedesktop/DBus','org.freedesktop.DBus','NameHasOwner',GLib.Variant('(s)',('org.gnome.keyring.SystemPrompter',)),GLib.VariantType.new('(b)'),Gio.DBusCallFlags.NONE,1000,None).unpack()[0]
        if owned:
            started=True;Gcr.SystemPrompt.open_async(3,None,opened,None)
    if result and prompt:prompt.close();prompt=None
    time.sleep(.005)
if child.poll() is None:child.kill()
out,err=child.communicate(timeout=3)
print(out,err)
assert child.returncode==0 and result==['Disposable-only-bridge-test'],'UI-to-keyring exchange failed'
print('PASS complete GCR -> native bridge -> morf UI -> GCR encrypted response')
