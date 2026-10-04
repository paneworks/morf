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
fn encoding_decompresses_and_archive_reads_a_tar() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "archive.lua",
            br##"
            local e, a = morf.encoding, morf.archive
            local gz = e.hex_decode("1f8b0800000000000003cb48cdc9c95748afca2ce00200397c63560b000000")
            assert(e.compression(gz) == "gzip")
            assert(e.decompress(gz) == "hello gzip\n")
            assert(e.decompress(gz, "gzip") == "hello gzip\n")
            local none, why = e.decompress(gz, "gzip", { max_size = 4 })
            assert(none == nil and why:find("exceeds"), why)
            assert(e.decompress("plain") == nil)
            assert(not pcall(e.decompress, gz, "gzip", { max_size = -1 }))
            assert(select(2, e.decompress(gz, "rar")):find("unknown"))

            -- `tar --format=gnu -c pkg-1.0-1 | zstd`: a directory and its desc.
            local db = e.hex_decode("28b52ffd04681d030042840e10907d842248fdaa9a3a88dbb1394f42a70e68bd4034c79ef8e04725730c422ae6295d5b8b3d040c75deee29f89ff5e63da5f55ecc786206ad370f20602d2c9603f1039fc0aaa6c3ec713e69c0064fa7300e00edff67c2f703071c146036f1398ce28a04")
            assert(e.compression(db) == "zstd")
            local list = assert(a.tar(db))
            assert(#list == 2, #list)
            assert(list[1].name == "pkg-1.0-1/" and list[1].type == "directory")
            assert(list[2].name == "pkg-1.0-1/desc" and list[2].type == "file" and list[2].size == 11, list[2].size)
            assert(list[2].data == nil)
            local full = a.tar(db, { contents = true })
            assert(full[2].data == "%NAME%\npkg\n", full[2].data)
            assert(a.tar_read(db, "pkg-1.0-1/desc") == full[2].data)
            local missing, err = a.tar_read(db, "nope")
            assert(missing == nil and err:find("no such"))
            local capped, cerr = a.tar(db, { max_entries = 1 })
            assert(capped == nil and cerr:find("more than 1"))
            assert(a.tar("") and #a.tar("") == 0)
            "##,
        )
        .unwrap();
}

#[test]
fn fs_reads_a_window_of_a_file() {
    let dir = scratch("window");
    let mut runtime = Runtime::default();
    let source = format!(
        r##"
        local fs = morf.fs
        local path = "{base}/log.txt"
        assert(fs.write(path, "0123456789"))
        assert(fs.read(path, {{ offset = 3, length = 4 }}) == "3456")
        assert(fs.read(path, {{ offset = 7 }}) == "789")
        assert(fs.read(path, {{ offset = 50 }}) == "")
        assert(fs.append(path, "abc"))
        local size = fs.stat(path).size
        assert(fs.read(path, {{ offset = size - 3 }}) == "abc")
        assert(not pcall(fs.read, path, {{ offset = -1 }}))
        "##,
        base = dir.display()
    );
    runtime.execute("window.lua", source.as_bytes()).unwrap();
    std::fs::remove_dir_all(&dir).unwrap();
}
