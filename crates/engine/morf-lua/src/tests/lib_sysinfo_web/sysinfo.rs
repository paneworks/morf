//! sysinfo: a fake machine in a folder, polling only while read, and this
//! machine.

use super::*;

/// A machine in a folder: two cores, a battery, a backlight anyone may write,
/// one real disk mounted twice, a tmpfs, two processes.
fn fake_machine(name: &str) -> std::path::PathBuf {
    let root = scratch(name);
    put(
        &root,
        "/proc/stat",
        "cpu  100 0 100 800 0 0 0 0 0 0\ncpu0 50 0 50 400 0 0 0 0 0 0\ncpu1 50 0 50 400 0 0 0 0 0 0\nintr 1 2 3\nctxt 5\n",
    );
    put(&root, "/proc/loadavg", "0.50 0.25 0.10 2/300 999\n");
    put(
        &root,
        "/proc/cpuinfo",
        "processor\t: 0\nmodel name\t: Test CPU 9000\ncpu MHz\t\t: 1000.000\n\nprocessor\t: 1\nmodel name\t: Test CPU 9000\ncpu MHz\t\t: 3000.000\n",
    );
    put(
        &root,
        "/proc/meminfo",
        "MemTotal:       1000 kB\nMemFree:         100 kB\nMemAvailable:    250 kB\nBuffers:          10 kB\nCached:          200 kB\nSwapTotal:       400 kB\nSwapFree:        300 kB\n",
    );
    put(
        &root,
        "/proc/mounts",
        "/dev/sda1 / ext4 rw 0 0\nproc /proc proc rw 0 0\ntmpfs /tmp tmpfs rw 0 0\n/dev/sdb2 /home/deep btrfs rw 0 0\n/dev/sdb2 /home btrfs rw 0 0\n/dev/loop0 /snap/x squashfs ro 0 0\n/dev/sdc1 /media/my\\040disk vfat rw 0 0\n",
    );
    std::fs::create_dir_all(root.join("home/deep")).unwrap();
    std::fs::create_dir_all(root.join("media/my disk")).unwrap();
    put(&root, "/proc/uptime", "3600.50 7000.00\n");
    put(&root, "/proc/sys/kernel/osrelease", "6.9.0-test\n");
    put(&root, "/proc/sys/kernel/hostname", "testbox\n");
    put(
        &root,
        "/etc/os-release",
        "NAME=\"Test\"\nPRETTY_NAME=\"Test OS 1\"\nID=test\nVERSION_ID=1\n",
    );
    put(
        &root,
        "/proc/net/dev",
        "Inter-|   Receive |  Transmit\n face |bytes packets errs drop fifo frame compressed multicast|bytes packets errs drop fifo colls carrier compressed\n    lo: 100 1 0 0 0 0 0 0 100 1 0 0 0 0 0 0\n  eth0: 1000 10 0 0 0 0 0 0 500 5 0 0 0 0 0 0\n",
    );
    put(
        &root,
        "/proc/net/route",
        "Iface\tDestination\tGateway \tFlags\tRefCnt\tUse\tMetric\tMask\t\tMTU\tWindow\tIRTT\neth0\t0000A8C0\t00000000\t0001\t0\t0\t100\t00FFFFFF\t0\t0\t0\neth0\t00000000\t0100A8C0\t0003\t0\t0\t100\t00000000\t0\t0\t0\n",
    );
    put(&root, "/sys/class/hwmon/hwmon0/name", "acpitz\n");
    put(&root, "/sys/class/hwmon/hwmon0/temp1_input", "40000\n");
    put(&root, "/sys/class/hwmon/hwmon1/name", "coretemp\n");
    put(&root, "/sys/class/hwmon/hwmon1/temp1_input", "55000\n");
    put(
        &root,
        "/sys/class/hwmon/hwmon1/temp1_label",
        "Package id 0\n",
    );
    put(&root, "/sys/class/hwmon/hwmon1/temp1_crit", "100000\n");
    put(&root, "/sys/class/hwmon/hwmon1/temp2_input", "53000\n");
    put(&root, "/sys/class/hwmon/hwmon1/temp2_label", "Core 0\n");
    put(
        &root,
        "/sys/class/thermal/thermal_zone0/type",
        "x86_pkg_temp\n",
    );
    put(&root, "/sys/class/thermal/thermal_zone0/temp", "54000\n");
    put(
        &root,
        "/sys/class/drm/card0/device/gpu_busy_percent",
        "37\n",
    );
    put(&root, "/sys/class/power_supply/AC/type", "Mains\n");
    put(&root, "/sys/class/power_supply/AC/online", "0\n");
    put(&root, "/sys/class/power_supply/BAT0/type", "Battery\n");
    put(
        &root,
        "/sys/class/power_supply/BAT0/status",
        "Discharging\n",
    );
    put(&root, "/sys/class/power_supply/BAT0/capacity", "50\n");
    put(
        &root,
        "/sys/class/power_supply/BAT0/energy_now",
        "25000000\n",
    );
    put(
        &root,
        "/sys/class/power_supply/BAT0/energy_full",
        "50000000\n",
    );
    put(
        &root,
        "/sys/class/power_supply/BAT0/power_now",
        "10000000\n",
    );
    put(&root, "/sys/class/backlight/panel/brightness", "200\n");
    put(&root, "/sys/class/backlight/panel/max_brightness", "400\n");
    put(&root, "/sys/class/backlight/dim/brightness", "1\n");
    put(&root, "/sys/class/backlight/dim/max_brightness", "10\n");
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(
        root.join("sys/class/backlight/panel/brightness"),
        std::fs::Permissions::from_mode(0o666),
    )
    .unwrap();
    std::fs::set_permissions(
        root.join("sys/class/backlight/dim/brightness"),
        std::fs::Permissions::from_mode(0o644),
    )
    .unwrap();
    put(
        &root,
        "/proc/self/status",
        "Name:\tmorf\nUid:\t1000\t1000\t1000\t1000\nGroups:\t1000\n",
    );
    put(&root, "/etc/group", "wheel:x:10:\n");
    put(
        &root,
        "/proc/12/stat",
        "12 (quiet one) S 1 12 12 0 -1 0 0 0 0 0 10 10 0 0 20 0 1 0 5 1000 100 0\n",
    );
    put(
        &root,
        "/proc/34/stat",
        "34 (busy (x)) R 1 34 34 0 -1 0 0 0 0 0 100 100 0 0 20 0 1 0 5 1000 50 0\n",
    );
    root
}

#[test]
fn sysinfo_reads_a_fake_machine() {
    let root = fake_machine("sysinfo");
    let text = run(
        &format!(
            r##"
            local sysinfo = require("lib.services.sysinfo")
            local fs = morf.fs
            local base = "{root}"
            sysinfo.configure {{ root = base, top = 5 }}

            local cpu = sysinfo.sample("cpu")
            assert(cpu.usage == 0 and cpu.count == 2, cpu.count)
            assert(cpu.model == "Test CPU 9000" and cpu.frequency == 2000, cpu.frequency)
            assert(cpu.load[1] == 0.5 and cpu.load[3] == 0.1 and cpu.threads == 300)
            -- 200 busy of 800 on the whole, 150 of 400 and 50 of 400 per core.
            assert(fs.write(base .. "/proc/stat",
              "cpu  300 0 100 1400 0 0 0 0 0 0\ncpu0 150 0 100 650 0 0 0 0 0 0\ncpu1 100 0 50 750 0 0 0 0 0 0\n"))
            cpu = sysinfo.sample("cpu")
            assert(cpu.usage == 25, cpu.usage)
            assert(cpu.cores[1].usage == 37.5 and cpu.cores[2].usage == 12.5, cpu.cores[1].usage)
            assert(cpu.cores[2].frequency == 3000)
            local history = sysinfo.history("cpu")
            assert(#history == 2 and history[2] == 25)

            local memory = sysinfo.sample("memory")
            assert(memory.total == 1024000 and memory.used == 750 * 1024 and memory.percent == 75)
            assert(memory.swap.used == 100 * 1024 and memory.swap.percent == 25)

            local disks = sysinfo.sample("disks")
            assert(#disks == 3, #disks)
            assert(disks[1].mount == "/" and disks[2].mount == "/home" and disks[3].mount == "/media/my disk",
              disks[2].mount)
            assert(disks[1].total > 0 and disks[1].percent >= 0)

            local temps = sysinfo.sample("temperatures")
            assert(temps.cpu == 55 and temps.cpu_sensor.label == "Package id 0")
            assert(#temps.sensors == 4, #temps.sensors)
            assert(temps.cpu_sensor.critical == 100)

            local gpu = sysinfo.sample("gpu")
            assert(gpu.busy == 37 and gpu.cards[1].name == "card0")

            local net = sysinfo.sample("network")
            assert(net.default == "eth0" and #net.interfaces == 1 and net.primary.rx_bytes == 1000)

            local battery = sysinfo.sample("battery")
            assert(battery.present and battery.percent == 50 and battery.status == "Discharging")
            assert(battery.power == 10 and battery.time_left == 9000 and battery.ac == false)

            local light = sysinfo.sample("backlight")
            local panel, dim
            for _, d in ipairs(light.devices) do
              if d.name == "panel" then panel = d else dim = d end
            end
            assert(panel.percent == 50 and panel.writable and not dim.writable)
            assert(sysinfo.set_brightness(25, "panel"))
            assert(fs.read(base .. "/sys/class/backlight/panel/brightness") == "100")
            assert(sysinfo.set_brightness(25, "dim") == nil)

            local system = sysinfo.sample("system")
            assert(system.os == "Test OS 1" and system.os_id == "test" and system.kernel == "6.9.0-test")
            assert(system.hostname == "testbox" and system.uptime == 3600.5)

            -- Processes come from a job, so from a binding.
            assert(not sysinfo.sources.processes:running())
            local stage = 0
            ui.Text {{ text = function()
              local processes = sysinfo.processes()
              if processes.count == 2 and stage == 0 then
                stage = 1
                assert(processes.by_memory[1].name == "quiet one", processes.by_memory[1].name)
                assert(processes.by_memory[1].memory == 100 * 4096)
                assert(processes.by_cpu[2].cpu == 0)
                morf.timer(1, function()
                  assert(sysinfo.sources.processes:running())
                  -- Ten more jiffies for "busy (x)" out of 800 more in all.
                  fs.write(base .. "/proc/34/stat",
                    "34 (busy (x)) R 1 34 34 0 -1 0 0 0 0 0 110 100 0 0 20 0 1 0 5 1000 50 0\n")
                  fs.write(base .. "/proc/stat",
                    "cpu  700 0 100 1800 0 0 0 0 0 0\ncpu0 1 1 1 1 0 0 0 0\ncpu1 1 1 1 1 0 0 0 0\n")
                  sysinfo.sources.processes:refresh()
                end, false)
              elseif stage == 1 and processes.by_cpu[1].cpu > 0 then
                stage = 2
                assert(processes.by_cpu[1].name == "busy (x)")
                -- 10 of 800 jiffies across two cores is 2.5% of one core.
                assert(processes.by_cpu[1].cpu == 2.5, processes.by_cpu[1].cpu)
                note("processes")
                note("done")
              end
              return ""
            end }}
            "##,
            root = root.display()
        ),
        10,
    );
    assert!(text.contains("processes;done;"), "{text}");
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn sysinfo_polls_only_while_read() {
    let root = fake_machine("sysinfo-idle");
    let text = run(
        &format!(
            r##"
            local sysinfo = require("lib.services.sysinfo")
            sysinfo.configure {{ root = "{root}", intervals = {{ memory = 20 }} }}
            local memory = sysinfo.sources.memory
            assert(not memory:running())
            -- A read from a handler, not a binding: nothing reads it again,
            -- so after a couple of samples the timer stops by itself.
            morf.timer(1, function()
              sysinfo.memory()
              assert(memory:running())
            end, false)
            local watch
            watch = morf.timer(10, function()
              if memory.samples >= 2 and not memory:running() then
                watch:cancel()
                note("idle after " .. memory.samples)
                note("done")
              end
            end, true)
            "##,
            root = root.display()
        ),
        5,
    );
    assert!(text.contains("done;"), "{text}");
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn sysinfo_reads_this_machine() {
    // Read-only: every section once, sanity only.
    let text = run(
        r##"
        local sysinfo = require("lib.services.sysinfo")
        local cpu = sysinfo.sample("cpu")
        assert(cpu.count >= 1 and cpu.usage >= 0 and cpu.usage <= 100)
        local memory = sysinfo.sample("memory")
        assert(memory.total > 0 and memory.percent >= 0 and memory.percent <= 100)
        assert(type(sysinfo.sample("disks")) == "table")
        assert(type(sysinfo.sample("temperatures").sensors) == "table")
        assert(type(sysinfo.sample("gpu").cards) == "table")
        assert(type(sysinfo.sample("network").interfaces) == "table")
        assert(type(sysinfo.sample("battery").present) == "boolean")
        assert(type(sysinfo.sample("backlight").devices) == "table")
        local system = sysinfo.sample("system")
        assert(system.kernel ~= "" and system.uptime > 0)
        local reported = false
        ui.Text { text = function()
          local processes = sysinfo.processes()
          local failed = sysinfo.sources.processes.error
          if (processes.count > 0 or failed) and not reported then
            reported = true
            morf.timer(1, function()
              note(failed or ("processes " .. processes.count))
              note("done")
            end, false)
          end
          return ""
        end }
        "##,
        20,
    );
    assert!(text.contains("done;"), "{text}");
    assert!(text.starts_with("processes "), "{text}");
}
