use super::*;

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("morf-lua-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn fs_module_lists_reads_writes_and_refuses_politely() {
    let dir = scratch("fs");
    let mut runtime = Runtime::default();
    let source = format!(
        r##"
        local fs = morf.fs
        local base = "{base}"
        assert(fs.write(fs.join(base, "notes", "a.md"), "# one\nbody\n"))
        assert(fs.append(fs.join(base, "notes", "a.md"), "more\n"))
        assert(fs.write(base .. "/w/tiger.jpeg", "x"))
        assert(fs.write(base .. "/w/cat.png", "yy"))
        assert(fs.read(base .. "/notes/a.md") == "# one\nbody\nmore\n")
        local lines = fs.lines(base .. "/notes/a.md")
        assert(#lines == 3 and lines[3] == "more", #lines)

        local entries, truncated = fs.list(base .. "/w")
        assert(#entries == 2 and truncated == false)
        assert(entries[1].name == "cat.png" and entries[1].size == 2 and entries[1].is_file)
        assert(entries[2].extension == "jpeg")
        local found = fs.glob(base .. "/w/*.{{jpeg,png}}")
        assert(#found == 2)
        assert(fs.matches("*.jpeg", "tiger.jpeg"))

        local missing, err = fs.read(base .. "/nope")
        assert(missing == nil and type(err) == "string")
        local stat = fs.stat(base .. "/notes")
        assert(stat.type == "dir" and stat.is_dir)
        assert(fs.exists(base .. "/w") and fs.is_dir(base .. "/w") and not fs.is_file(base .. "/w"))

        assert(fs.copy(base .. "/w", base .. "/w2", {{ recursive = true }}) == 3)
        assert(fs.rename(base .. "/w2/cat.png", base .. "/w2/dog.png"))
        assert(fs.exists(base .. "/w2/dog.png"))
        local refused = fs.remove(base .. "/w2")
        assert(refused == nil)
        assert(fs.remove(base .. "/w2", {{ recursive = true }}))
        assert(not fs.exists(base .. "/w2"))
        assert(fs.remove("/") == nil)

        assert(fs.basename("/a/b.tar.gz") == "b.tar.gz")
        assert(fs.dirname("/a/b") == "/a" and fs.dirname("b") == ".")
        assert(fs.extension("x.JPEG") == "JPEG" and fs.stem("x.png") == "x")
        assert(fs.expand("~/x") == fs.home() .. "/x")
        assert(fs.normalize("/a/./b/../c") == "/a/c")
        assert(type(fs.dir("config")) == "string")
        assert(fs.disk(base).total > 0)
        assert(not pcall(fs.read, 42))
        assert(not pcall(fs.list, base, {{ depth = "deep" }}))
        "##,
        base = dir.display()
    );
    runtime.execute("fs.lua", source.as_bytes()).unwrap();
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn time_module_does_calendar_arithmetic() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "time.lua",
            br##"
            local t = morf.time
            local utc = { tz = "UTC" }
            local now = t.now()
            assert(now > 1.7e9 and math.abs(t.now_ms() / 1000 - now) < 5)

            local epoch = t.time({ year = 2024, month = 2, day = 29, hour = 13, minute = 5 }, utc)
            assert(epoch == 1709211900, epoch)
            local d = t.date(epoch, utc)
            assert(d.year == 2024 and d.month == 2 and d.day == 29 and d.hour == 13)
            assert(d.weekday == 4 and d.leap_year and d.days_in_month == 29 and d.yearday == 60)
            assert(t.format("%Y-%m-%d %H:%M %a", epoch, utc) == "2024-02-29 13:05 Thu")

            -- A month from Jan 31 is the end of February, not March 2.
            local jan31 = t.time({ year = 2023, month = 1, day = 31 }, utc)
            assert(t.format("%F", t.add(jan31, { months = 1 }, utc), utc) == "2023-02-28")
            assert(t.format("%F", t.add(jan31, { days = -1 }, utc), utc) == "2023-01-30")

            assert(t.parse("2024-02-29T13:05:00Z") == epoch)
            assert(t.parse("2024-02-29 13:05", "%Y-%m-%d %H:%M", utc) == epoch)
            assert(t.parse("2024-02-29", nil, utc) == t.start_of(epoch, "day", utc))
            local bad, err = t.parse("not a date")
            assert(bad == nil and type(err) == "string")

            assert(t.format("%F", t.start_of(epoch, "week", utc), utc) == "2024-02-26")
            assert(t.format("%F", t.start_of(epoch, "month", utc), utc) == "2024-02-01")
            assert(t.days_between(jan31, epoch, utc) == 394)
            assert(t.days_in_month(2023, 2) == 28 and t.is_leap_year(2000) and not t.is_leap_year(1900))
            assert(t.weekday(2024, 9, 24) == 2)

            local grid = t.month(2024, 2)
            assert(#grid == 5 and #grid[1] == 7)
            assert(grid[1][1].day == 29 and grid[1][1].current == false)
            assert(grid[1][4].day == 1 and grid[1][4].current)

            assert(t.relative(now - 300, now) == "5 minutes ago")
            assert(t.duration(3909) == "1:05:09" and t.duration(3909, "short") == "1h 5m")
            assert(type(t.timezone()) == "string")
            assert(not pcall(t.format, "%Y", 0, { tz = "Nowhere/Void" }))
            assert(not pcall(t.start_of, epoch, "fortnight"))
            "##,
        )
        .unwrap();
}

#[test]
fn encoding_module_round_trips_and_digests() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "encoding.lua",
            br##"
            local e = morf.encoding
            assert(e.base64_encode("foobar") == "Zm9vYmFy")
            assert(e.base64_encode("\251\255", { url = true }) == "-_8")
            assert(e.base64_decode("Zm9vYg==") == "foob")
            local bad, err = e.base64_decode("!!")
            assert(bad == nil and err:find("base64"))
            assert(e.hex_encode("\0\171\255") == "00abff" and e.hex_encode("\255", { upper = true }) == "FF")
            assert(e.hex_decode("00abff") == "\0\171\255")
            assert(e.url_encode("a b&c") == "a%20b%26c")
            assert(e.url_encode("a b/c", { component = false, plus = true }) == "a+b/c")
            assert(e.url_decode("a+b%2F", { plus = true }) == "a b/")
            assert(e.sha256("abc") == "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")
            assert(#e.sha256("abc", true) == 32)
            assert(e.sha1("abc") == "a9993e364706816aba3e25717850c26c9cd0d89d")
            assert(e.crc32("123456789") == 0xCBF43926)
            assert(#e.random_bytes(24) == 24)
            local id = e.uuid()
            assert(#id == 36 and id:sub(15, 15) == "4")
            assert(not pcall(e.random_bytes, -1))
            assert(not pcall(e.sha256, {}))
            "##,
        )
        .unwrap();
}

#[test]
fn log_writes_levels_and_stays_bounded() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "log.lua",
            br#"
            morf.log("warn", "settings", "missing", 3)
            morf.log.info("hello")
            assert(not pcall(morf.log, "loud", "x"))
            for i = 1, 2500 do morf.log.debug("line", i) end
            "#,
        )
        .unwrap();
    let logs = runtime.take_logs();
    assert_eq!(logs.len(), crate::state::MAX_LOG_ENTRIES);
    assert_eq!(logs.last().unwrap().message, "line 2500");
    assert!(
        logs.iter()
            .all(|entry| entry.message != "settings missing 3")
    );
    let mut fresh = Runtime::default();
    fresh
        .execute("log2.lua", br#"morf.log("warn", "settings", "missing", 3)"#)
        .unwrap();
    let logs = fresh.take_logs();
    assert!(
        logs.iter()
            .any(|e| e.message == "settings missing 3" && e.level == LogLevel::Warn),
        "{logs:?}"
    );
}
