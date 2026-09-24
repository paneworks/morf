//! The pure-Lua libraries in `examples/lib` that watch the machine and the
//! web: sysinfo, weather, github, packages, claude_usage.
//!
//! Each runs inside a real runtime with `examples/` as a module root. The
//! machine is a folder of fake /proc and /sys files, the web is a server on
//! loopback serving recorded answers, the package tools are an injected
//! runner, and the transcripts are written by the test. Nothing here needs
//! the network or changes the system.

use std::time::{Duration, Instant};

use super::*;

fn examples_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples")
}

/// Runs `source` with `examples/` as a module root and pumps the loop until
/// the `seen` text contains `done` (or the deadline passes). Returns the text.
/// Any log line -- a handler that raised or ran out of fuel -- fails the test.
fn run(source: &str, seconds: u64) -> String {
    let mut runtime = Runtime::default();
    runtime.set_module_roots(vec![examples_dir()]);
    let source = format!(
        r#"
        local morf = require("morf")
        local ui = require("morf.ui")
        local seen = morf.signal("test.seen", "")
        local function note(text) seen:set(seen:get() .. tostring(text) .. ";") end
        {source}
        ui.Text {{ text = function() return seen:get() end }}
        "#
    );
    runtime.execute("lib_test.lua", source.as_bytes()).unwrap();
    let root = *runtime.scene().roots().last().unwrap();
    let deadline = Instant::now() + Duration::from_secs(seconds);
    loop {
        runtime.poll_services();
        let text = runtime
            .scene()
            .string_value(root, "text")
            .unwrap()
            .to_owned();
        let logs = runtime.take_logs();
        assert!(logs.is_empty(), "{logs:?}\n{text}");
        if text.contains("done;") || Instant::now() > deadline {
            return text;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("morf-lib-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn put(root: &std::path::Path, path: &str, text: &str) {
    let path = root.join(path.trim_start_matches('/'));
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

/// What the loopback server answers: a path prefix and a status and body.
type Routes = Vec<(&'static str, u16, String)>;

/// A server on loopback answering each request by the first route whose
/// prefix its path starts with, 404 otherwise. Returns its base URL and the
/// paths (with queries) it was asked for, in order.
fn serve(routes: Routes) -> (String, Arc<Mutex<Vec<String>>>) {
    use std::io::{BufRead, BufReader, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let hits = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::clone(&hits);
    let routes = Arc::new(routes);
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let routes = Arc::clone(&routes);
            let seen = Arc::clone(&seen);
            std::thread::spawn(move || {
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                let _ = reader.read_line(&mut line);
                let path = line.split_whitespace().nth(1).unwrap_or("").to_owned();
                let mut length = 0;
                loop {
                    let mut header = String::new();
                    if reader.read_line(&mut header).unwrap_or(0) == 0 || header.trim().is_empty() {
                        break;
                    }
                    if let Some((name, value)) = header.split_once(':')
                        && name.trim().eq_ignore_ascii_case("content-length")
                    {
                        length = value.trim().parse().unwrap_or(0);
                    }
                }
                let mut body = vec![0; length];
                let _ = std::io::Read::read_exact(&mut reader, &mut body);
                seen.lock().unwrap().push(if body.is_empty() {
                    path.clone()
                } else {
                    format!("{path} {}", String::from_utf8_lossy(&body))
                });
                let (status, body) = routes
                    .iter()
                    .find(|(prefix, _, _)| path.starts_with(prefix))
                    .map_or((404, String::new()), |(_, status, body)| {
                        (*status, body.clone())
                    });
                let mut stream = stream;
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status} X\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.flush();
            });
        }
    });
    (format!("http://{address}"), hits)
}

use std::sync::{Arc, Mutex};

fn now_seconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

// ---------------------------------------------------------------------------
// weather

/// An Open-Meteo answer shaped like the real one, with hours from an hour ago
/// for two days so "the next 24 hours" has something to choose from.
fn open_meteo_answer() -> String {
    let hour = now_seconds() / 3600 * 3600;
    let times: Vec<i64> = (-1..47).map(|index| hour + index * 3600).collect();
    let temps: Vec<f64> = (0..48).map(|index| 10.0 + index as f64 * 0.5).collect();
    let codes: Vec<i64> = (0..48)
        .map(|index| if index < 6 { 61 } else { 0 })
        .collect();
    let days: Vec<i64> = (0..7).map(|index| hour + index * 86400).collect();
    serde_json::json!({
        "timezone": "Europe/Amsterdam",
        "current": {
            "time": hour, "temperature_2m": 14.2, "relative_humidity_2m": 88,
            "apparent_temperature": 13.1, "is_day": 0, "weather_code": 3,
            "wind_speed_10m": 12.6, "wind_direction_10m": 313
        },
        "hourly": {
            "time": times, "temperature_2m": temps, "weather_code": codes,
            "precipitation_probability": vec![40; 48], "is_day": vec![1; 48]
        },
        "daily": {
            "time": days, "weather_code": [61, 0, 1, 2, 3, 95, 71],
            "temperature_2m_max": [18.0, 19.0, 20.0, 21.0, 22.0, 23.0, 24.0],
            "temperature_2m_min": [11.0, 12.0, 13.0, 14.0, 15.0, 16.0, 17.0],
            "sunrise": days, "sunset": days,
            "precipitation_probability_max": [80, 0, 0, 10, 20, 90, 50]
        }
    })
    .to_string()
}

fn wttr_answer() -> String {
    serde_json::json!({
        "current_condition": [{
            "FeelsLikeC": "11", "FeelsLikeF": "52", "humidity": "96", "temp_C": "12",
            "temp_F": "54", "weatherCode": "143", "weatherDesc": [{"value": "Mist"}],
            "winddir16Point": "WNW", "winddirDegree": "302", "windspeedKmph": "10",
            "windspeedMiles": "6"
        }],
        "nearest_area": [{"areaName": [{"value": "Wageningen"}], "country": [{"value": "Netherlands"}]}],
        "weather": [{
            "date": "2099-09-24", "maxtempC": "18", "maxtempF": "64", "mintempC": "11", "mintempF": "52",
            "hourly": [
                {"time": "0", "tempC": "12", "tempF": "54", "weatherCode": "113", "chanceofrain": "0"},
                {"time": "1200", "tempC": "17", "tempF": "63", "weatherCode": "296", "chanceofrain": "70"}
            ]
        }]
    })
    .to_string()
}

#[test]
fn weather_from_open_meteo_with_a_geocoded_place_and_a_cache() {
    let cache = scratch("weather");
    let geocode = r#"{"results":[{"name":"Wageningen","latitude":51.97,"longitude":5.66667,"country":"The Netherlands"}]}"#;
    let (base, hits) = serve(vec![
        ("/v1/search", 200, geocode.to_owned()),
        ("/v1/forecast", 200, open_meteo_answer()),
    ]);
    let text = run(
        &format!(
            r##"
            local weather = require("lib.weather")
            local here = weather.new {{
              location = "Wageningen", cache_dir = "{cache}",
              geocoding_url = "{base}/v1/search", forecast_url = "{base}/v1/forecast",
              wttr_url = "{base}/wttr",
            }}
            local reported = false
            ui.Text {{ text = function()
              local now = here:get()
              if now.available and not reported then
                reported = true
                assert(now.source == "open-meteo" and now.temperature == 14.2 and now.feels_like == 13.1)
                assert(now.humidity == 88 and now.wind_speed == 12.6 and now.wind_direction == 313)
                assert(now.condition == "Overcast" and now.icon == "weather-overcast" and now.glyph == "☁")
                assert(now.is_day == false and now.high == 18 and now.low == 11)
                assert(now.place == "Wageningen, The Netherlands", now.place)
                assert(#now.hourly == 24, #now.hourly)
                assert(now.hourly[1].condition == "Light rain" and now.hourly[1].temperature == 10.5)
                assert(#now.daily == 7 and now.daily[6].condition == "Thunderstorm")
                assert(now.daily[7].icon == "weather-snow")
                assert(now.units.temperature == "°C" and now.units.wind == "km/h")
                morf.timer(1, function()
                  -- A second instance for the same place reads the cache and
                  -- asks nobody.
                  local again = weather.new {{ location = "Wageningen", cache_dir = "{cache}",
                    geocoding_url = "http://127.0.0.1:9/", forecast_url = "http://127.0.0.1:9/" }}
                  local noted = false
                  ui.Text {{ text = function()
                    local value = again:get()
                    if value.available and not noted then
                      noted = true
                      morf.timer(1, function() note(value.temperature) note("done") end, false)
                    end
                    return ""
                  end }}
                end, false)
              end
              return ""
            end }}
            "##,
            cache = cache.display(),
        ),
        10,
    );
    assert!(text.contains("14.2;done;"), "{text}");
    let hits = hits.lock().unwrap().clone();
    assert_eq!(hits.len(), 2, "{hits:?}");
    assert!(hits[0].starts_with("/v1/search?") && hits[0].contains("name=Wageningen"));
    assert!(hits[1].contains("latitude=51.97") && hits[1].contains("timeformat=unixtime"));
    std::fs::remove_dir_all(&cache).unwrap();
}

#[test]
fn weather_falls_back_to_wttr_and_keeps_the_last_answer_offline() {
    let cache = scratch("weather-fallback");
    let (base, hits) = serve(vec![
        ("/v1/forecast", 503, "down".to_owned()),
        ("/wttr/", 200, wttr_answer()),
    ]);
    let text = run(
        &format!(
            r##"
            local weather = require("lib.weather")
            local here = weather.new {{
              latitude = 51.97, longitude = 5.66, units = "imperial", cache_dir = "{cache}",
              forecast_url = "{base}/v1/forecast", wttr_url = "{base}/wttr",
            }}
            local stage = 0
            ui.Text {{ text = function()
              local now = here:get()
              if now.available and stage == 0 then
                stage = 1
                assert(now.source == "wttr.in" and now.temperature == 54 and now.feels_like == 52)
                assert(now.condition == "Mist" and now.icon == "weather-fog" and now.wind_speed == 6)
                assert(now.place == "Wageningen, Netherlands" and now.high == 64 and now.low == 52)
                assert(#now.hourly == 2 and now.hourly[2].condition == "Light rain")
                assert(now.units.temperature == "°F")
                assert(not now.stale)
                morf.timer(1, function()
                  -- Everything down: the last answer comes back, stale.
                  here.forecast_url = "http://127.0.0.1:9/"
                  here.wttr_url = "http://127.0.0.1:9"
                  here:refresh()
                end, false)
              elseif stage == 1 and now.stale then
                stage = 2
                morf.timer(1, function() note("stale " .. now.temperature) note("done") end, false)
              end
              return ""
            end }}
            "##,
            cache = cache.display(),
        ),
        30,
    );
    assert!(text.contains("stale 54;done;"), "{text}");
    let hits = hits.lock().unwrap().clone();
    assert!(hits[0].contains("temperature_unit=fahrenheit"), "{hits:?}");
    assert!(
        hits[1].starts_with("/wttr/51.97%2C5.66?format=j1"),
        "{hits:?}"
    );
    std::fs::remove_dir_all(&cache).unwrap();
}

#[test]
fn weather_codes_read_as_words_icons_and_glyphs() {
    let text = run(
        r##"
        local weather = require("lib.weather")
        local clear = weather.condition(0, true)
        assert(clear.text == "Clear" and clear.icon == "weather-clear" and clear.glyph == "☀")
        local night = weather.condition(0, false)
        assert(night.icon == "weather-clear-night" and night.glyph == "🌙")
        assert(weather.condition(99).text == "Thunderstorm with hail")
        assert(weather.condition(1234).icon == "weather-severe-alert")
        note("done")
        "##,
        2,
    );
    assert!(text.contains("done;"), "{text}");
}

// ---------------------------------------------------------------------------
// github

/// A contributions page for 2026-01-04 (a Sunday) to 2026-01-20, the shape
/// GitHub renders: cells with `data-date` and `data-level`, tooltips naming
/// the count by the cell's id. One cell has its attributes in another order.
fn contributions_page() -> String {
    let counts: Vec<i64> = vec![1, 2, 3, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 5, 1200, 0];
    let mut html = String::from(
        "<html><h2 id=\"js-contribution-activity-description\">\n  1,337\n  contributions\n  in the last year\n</h2><table>",
    );
    // Row by row, as GitHub draws it: every Sunday, then every Monday.
    let mut order: Vec<usize> = (0..counts.len()).collect();
    order.sort_by_key(|index| (index % 7, index / 7));
    for index in order {
        let count = &counts[index];
        let date = format!("2026-01-{:02}", index + 4);
        let level = (*count).min(4);
        let id = format!("contribution-day-component-{}-{}", index % 7, index / 7);
        if index == 1 {
            html.push_str(&format!(
                "<td id=\"{id}\" data-level=\"{level}\" class=\"ContributionCalendar-day\" data-date=\"{date}\"></td>"
            ));
        } else {
            html.push_str(&format!(
                "<td tabindex=\"0\" data-ix=\"{index}\" style=\"width: 10px\" data-date=\"{date}\" id=\"{id}\" data-level=\"{level}\" role=\"gridcell\" class=\"ContributionCalendar-day\"></td>"
            ));
        }
        let words = match count {
            0 => format!("No contributions on January {}th.", index + 4),
            1 => format!("1 contribution on January {}th.", index + 4),
            1200 => format!("1,200 contributions on January {}th.", index + 4),
            n => format!("{n} contributions on January {}th.", index + 4),
        };
        html.push_str(&format!(
            "<tool-tip id=\"tooltip-{index}\" for=\"{id}\" popover=\"manual\" class=\"sr-only\">{words}</tool-tip>"
        ));
    }
    html.push_str("</table></html>");
    html
}

#[test]
fn github_reads_the_contributions_page() {
    let cache = scratch("github");
    let (base, hits) = serve(vec![(
        "/users/octo/contributions",
        200,
        contributions_page(),
    )]);
    let text = run(
        &format!(
            r##"
            local github = require("lib.github")
            local octo = github.new {{ user = "octo", base_url = "{base}", cache_dir = "{cache}",
              today = function() return "2026-01-20" end }}
            local reported = false
            ui.Text {{ text = function()
              local calendar = octo:get()
              if calendar.available and not reported then
                reported = true
                assert(#calendar.days == 17, #calendar.days)
                assert(calendar.days[1].date == "2026-01-04" and calendar.days[1].count == 1)
                assert(calendar.days[1].weekday == 0 and calendar.days[2].weekday == 1)
                assert(calendar.days[2].count == 2 and calendar.days[2].level == 2)
                assert(calendar.days[16].count == 1200 and calendar.days[16].level == 4)
                assert(calendar.total == 1337, calendar.total)
                assert(calendar.longest_streak == 9, calendar.longest_streak)
                assert(calendar.current_streak == 2, calendar.current_streak)
                assert(calendar.today == 0 and calendar.max == 1200)
                assert(#calendar.weeks == 3 and calendar.weeks[3][1].date == "2026-01-18")
                assert(calendar.source == "page")
                morf.timer(1, function()
                  -- Fresh in the cache: a second reader asks nobody.
                  local again = github.new {{ user = "octo", base_url = "http://127.0.0.1:9",
                    cache_dir = "{cache}", today = function() return "2026-01-19" end }}
                  local seen_again = false
                  ui.Text {{ text = function()
                    local value = again:get()
                    if value.available and not seen_again then
                      seen_again = true
                      -- Yesterday's view: 1200 today, a streak of two.
                      morf.timer(1, function()
                        note(value.total .. " " .. value.current_streak .. " " .. value.today)
                        note("done")
                      end, false)
                    end
                    return ""
                  end }}
                end, false)
              end
              return ""
            end }}
            "##,
            cache = cache.display(),
        ),
        10,
    );
    assert!(text.contains("1337 2 1200;done;"), "{text}");
    assert_eq!(hits.lock().unwrap().len(), 1);
    std::fs::remove_dir_all(&cache).unwrap();
}

#[test]
fn github_asks_graphql_with_a_token() {
    let cache = scratch("github-graphql");
    let answer = serde_json::json!({"data": {"user": {"contributionsCollection": {"contributionCalendar": {
        "totalContributions": 7,
        "weeks": [
            {"contributionDays": [
                {"date": "2026-01-18", "contributionCount": 3, "contributionLevel": "SECOND_QUARTILE"},
                {"date": "2026-01-19", "contributionCount": 4, "contributionLevel": "FOURTH_QUARTILE"},
                {"date": "2026-01-20", "contributionCount": 0, "contributionLevel": "NONE"}
            ]}
        ]
    }}}}})
    .to_string();
    let (base, hits) = serve(vec![("/graphql", 200, answer)]);
    let text = run(
        &format!(
            r##"
            local github = require("lib.github")
            local octo = github.new {{ user = "octo", token = "secret", api_url = "{base}/graphql",
              cache_dir = "{cache}", today = function() return "2026-01-20" end }}
            local reported = false
            ui.Text {{ text = function()
              local calendar = octo:get()
              if calendar.available and not reported then
                reported = true
                local d = calendar.days
                morf.timer(1, function()
                  note(calendar.source .. " " .. calendar.total .. " " .. #d .. " " .. d[2].level .. " "
                    .. calendar.current_streak .. " " .. calendar.longest_streak)
                  note("done")
                end, false)
              end
              return ""
            end }}
            "##,
            cache = cache.display(),
        ),
        10,
    );
    assert!(text.contains("graphql 7 3 4 2 2;done;"), "{text}");
    let hits = hits.lock().unwrap().clone();
    assert!(
        hits[0].starts_with("/graphql ") && hits[0].contains("\"login\":\"octo\""),
        "{hits:?}"
    );
    std::fs::remove_dir_all(&cache).unwrap();
}

// ---------------------------------------------------------------------------
// packages

#[test]
fn packages_compares_versions_as_pacman_does() {
    let text = run(
        r##"
        local packages = require("lib.packages")
        local v = packages.vercmp
        assert(v("1.0", "1.0") == 0)
        assert(v("1.0", "1.1") == -1 and v("1.1", "1.0") == 1)
        assert(v("1.0-2", "1.0-1") == 1)
        assert(v("1:0.1", "2.0") == 1)
        assert(v("1.0a", "1.0") == -1 and v("1.0", "1.0a") == 1)
        assert(v("1.0", "1.0.1") == -1)
        assert(v("1.10", "1.9") == 1)
        assert(v("1.0.0.r15.gabc-1", "1.0.0.r9.gdef-1") == 1)
        assert(v("2.38.1-1", "2.38.1-1") == 0)
        assert(v("0.9.9-1", "1.0.0-1") == -1)
        note("done")
        "##,
        2,
    );
    assert!(text.contains("done;"), "{text}");
}

#[test]
fn packages_asks_pacman_the_aur_and_flatpak() {
    let aur = serde_json::json!({"resultcount": 2, "type": "multiinfo", "results": [
        {"Name": "yay", "Version": "12.4.2-1"},
        {"Name": "paru", "Version": "2.0.3-1"}
    ]})
    .to_string();
    let (base, hits) = serve(vec![("/rpc/v5/info", 200, aur)]);
    let text = run(
        &format!(
            r##"
            local packages = require("lib.packages")
            local ran = {{}}
            local answers = {{
              ["/usr/bin/pacman -Q"] = {{ ok = true, code = 0, stdout = "a 1-1\nb 2-1\nyay 12.3.0-1\nparu 2.0.3-1\n", stderr = "" }},
              ["/usr/bin/checkupdates"] = {{ ok = true, code = 0,
                stdout = "linux 6.9.1.arch1-1 -> 6.9.2.arch1-1\nmesa 1:24.1.0-1 -> 1:24.1.1-1\n", stderr = "" }},
              ["/usr/bin/pacman -Qm"] = {{ ok = true, code = 0, stdout = "yay 12.3.0-1\nparu 2.0.3-1\n", stderr = "" }},
              ["/usr/bin/flatpak list --columns=application"] = {{ ok = true, code = 0, stdout = "org.a.App\norg.b.App\norg.c.App\n", stderr = "" }},
              ["/usr/bin/flatpak remote-ls --updates --columns=application,version"] = {{ ok = true, code = 0, stdout = "org.b.App\t3.1\n", stderr = "" }},
            }}
            local checker = packages.new {{
              aur_url = "{base}/rpc/v5/info",
              which = function(name) return "/usr/bin/" .. name end,
              run = function(argv, on_done)
                local key = table.concat(argv, " ")
                ran[#ran + 1] = key
                local answer = answers[key] or {{ ok = false, code = 1, stdout = "", stderr = "unknown " .. key }}
                morf.timer(1, function() on_done(answer) end, false)
              end,
            }}
            local reported = false
            ui.Text {{ text = function()
              local state = checker:get()
              if not state.checking and not reported then
                reported = true
                assert(#state.errors == 0, state.errors[1])
                assert(table.concat(state.managers, ",") == "pacman,aur,flatpak")
                assert(state.pacman.installed == 4 and state.pacman.count == 2 and state.pacman.via == "checkupdates")
                assert(state.pacman.updates[2].name == "mesa" and state.pacman.updates[2].new == "1:24.1.1-1")
                assert(state.aur.foreign == 2 and state.aur.count == 1)
                assert(state.aur.updates[1].name == "yay" and state.aur.updates[1].new == "12.4.2-1")
                assert(state.flatpak.installed == 3 and state.flatpak.count == 1)
                assert(state.flatpak.updates[1].name == "org.b.App" and state.flatpak.updates[1].new == "3.1")
                morf.timer(1, function()
                  note(state.total)
                  note(table.concat(ran, "|"))
                  note("done")
                end, false)
              end
              return ""
            end }}
            "##
        ),
        10,
    );
    assert!(text.starts_with("4;"), "{text}");
    // Only queries, and nothing through a shell.
    for command in text.split(';').nth(1).unwrap().split('|') {
        assert!(
            command == "/usr/bin/pacman -Q"
                || command == "/usr/bin/checkupdates"
                || command == "/usr/bin/pacman -Qm"
                || command.starts_with("/usr/bin/flatpak list")
                || command.starts_with("/usr/bin/flatpak remote-ls"),
            "{command}"
        );
    }
    let hits = hits.lock().unwrap().clone();
    assert_eq!(hits.len(), 1);
    assert!(
        hits[0].contains("arg[]=yay") && hits[0].contains("arg[]=paru"),
        "{hits:?}"
    );
}

#[test]
fn packages_without_checkupdates_falls_back_to_pacman_qu() {
    let text = run(
        r##"
        local packages = require("lib.packages")
        local checker = packages.new {
          aur = false,
          which = function(name) if name == "pacman" then return "/usr/bin/pacman" end end,
          run = function(argv, on_done)
            local key = table.concat(argv, " ")
            local answer
            if key == "/usr/bin/pacman -Q" then
              answer = { ok = true, code = 0, stdout = "a 1-1\n", stderr = "" }
            elseif key == "/usr/bin/pacman -Qu" then
              -- pacman -Qu says "nothing" by exiting 1 with no output.
              answer = { ok = false, code = 1, stdout = "", stderr = "" }
            end
            morf.timer(1, function() on_done(answer) end, false)
          end,
        }
        local reported = false
        ui.Text { text = function()
          local state = checker:get()
          if not state.checking and not reported then
            reported = true
            morf.timer(1, function()
              note(state.pacman.via .. " " .. state.pacman.count .. " " .. state.total .. " " .. #state.errors
                .. " " .. tostring(state.flatpak) .. " " .. tostring(state.aur))
              note("done")
            end, false)
          end
          return ""
        end }
        "##,
        10,
    );
    assert!(text.contains("pacman -Qu 0 0 0 nil nil;done;"), "{text}");
}

// ---------------------------------------------------------------------------
// poll

#[test]
fn poll_runs_programs_directly_and_jobs_in_slices() {
    let text = run(
        r##"
        local poll = require("lib.poll")
        local echo = poll.which("echo")
        assert(echo and echo:sub(1, 1) == "/", tostring(echo))
        assert(poll.which("no-such-program-anywhere") == nil)
        local ring = poll.ring(3)
        for i = 1, 5 do ring.push(i) end
        assert(table.concat(ring.list(), ",") == "3,4,5")
        local pending = 3
        local function finished() pending = pending - 1 if pending == 0 then note("done") end end
        -- The argument is one argument: no shell splits or expands it.
        poll.run({ echo, "a b $HOME" }, function(result)
          assert(result.ok and result.code == 0, result.error)
          note(result.stdout:gsub("\n", ""))
          finished()
        end)
        poll.run({ "/no/such/program" }, function(result)
          assert(not result.ok and result.error)
          finished()
        end)
        -- Twenty thousand calls: five handlers' worth, done in slices.
        poll.job(function(spend)
          local sum = 0
          for i = 1, 20000 do sum = sum + math.abs(-i) spend(1) end
          return sum
        end, function(sum)
          note(sum)
          finished()
        end)
        "##,
        10,
    );
    assert!(text.contains("a b $HOME;"), "{text}");
    assert!(text.contains("200010000;"), "{text}");
    assert!(text.contains("done;"), "{text}");
}

// ---------------------------------------------------------------------------
// claude_usage

/// One transcript line the way Claude Code writes it: compact JSON, the
/// usage inside the message (with the per-iteration copy after it), then the
/// request id, the type and the timestamp.
fn assistant_line(request: &str, stamp: &str, input: u64, output: u64, created: u64) -> String {
    serde_json::json!({
        "parentUuid": "p",
        "message": {
            "model": "claude", "id": format!("msg_{request}"), "type": "message", "role": "assistant",
            "content": [{"type": "text", "text": "a \"usage\":{ that is only text, and ünïcödé"}],
            "usage": {
                "input_tokens": input, "cache_creation_input_tokens": created,
                "cache_read_input_tokens": 99999, "output_tokens": output,
                "output_tokens_details": {"thinking_tokens": 1},
                "iterations": [{"input_tokens": 777777, "output_tokens": 777777}]
            }
        },
        "requestId": request,
        "type": "assistant",
        "uuid": "u",
        "timestamp": stamp
    })
    .to_string()
        + "\n"
}

fn transcripts(root: &std::path::Path) {
    let user = serde_json::json!({"type": "user", "message": {"role": "user", "content": "hi"},
        "toolUseResult": {"usage": {"input_tokens": 555555, "output_tokens": 1}}, "timestamp": "2026-01-20T12:00:00Z"})
    .to_string()
        + "\n";
    let a = [
        user.clone(),
        assistant_line("r1", "2026-01-20T10:15:00.000Z", 100, 50, 10),
        assistant_line("r2", "2026-01-20T12:05:00.000Z", 400, 100, 500),
        // The same turn again, as a second content block: counted once.
        assistant_line("r2", "2026-01-20T12:05:01.000Z", 400, 100, 500),
        user,
    ]
    .concat();
    put(root, "/projects/-home-a/one.jsonl", &a);
    let b = [
        assistant_line("r3", "2026-01-17T09:00:00.000Z", 4000, 1000, 0),
        assistant_line("r4", "2026-01-01T00:30:00.000Z", 7, 0, 0),
    ]
    .concat();
    put(root, "/projects/-home-b/two/subagents/agent-x.jsonl", &b);
}

fn usage_script(root: &std::path::Path, extra: &str) -> String {
    format!(
        r##"
        local claude_usage = require("lib.claude_usage")
        local now = claude_usage.hour_of("2026-01-20T12:00:00Z") * 3600 + 1800
        local root = "{root}"
        local function make()
          return claude_usage.new {{ dir = root .. "/projects", state_path = root .. "/state.json",
            now = function() return now end, keep_hours = 24 * 30 {extra} }}
        end
        local usage = make()
        local stage = 0
        local function report(u)
          return u.block_tokens .. " " .. u.block_messages .. " " .. u.week_tokens .. " " .. u.week_messages
            .. " " .. u.peak_block_tokens .. " " .. u.peak_week_tokens .. " " .. u.files
        end
        ui.Text {{ text = function()
          local u = usage:get()
          if u.available and not u.scanning and stage == 0 then
            stage = 1
            assert(u.block_start == claude_usage.hour_of("2026-01-20T10:00:00Z") * 3600, u.block_start)
            assert(u.block_end == u.block_start + 5 * 3600)
            local first = report(u)
            morf.timer(1, function()
              note(first)
              -- A new turn is appended; the next pass reads only it.
              assert(morf.fs.append(root .. "/projects/-home-a/one.jsonl",
                {line}))
              usage:refresh()
            end, false)
          elseif stage == 1 and u.block_tokens ~= 1160 then
            stage = 2
            local second = report(u)
            morf.timer(1, function()
              note(second)
              -- A reader that starts from the kept state counts nothing twice.
              local again = make()
              local done_again = false
              ui.Text {{ text = function()
                local v = again:get()
                if v.available and not v.scanning and not done_again then
                  done_again = true
                  morf.timer(1, function() note(report(v)) note("done") end, false)
                end
                return ""
              end }}
            end, false)
          end
          return ""
        end }}
        "##,
        root = root.display(),
        extra = extra,
        line =
            serde_json::to_string(&assistant_line("r5", "2026-01-20T12:20:00Z", 1, 0, 0)).unwrap(),
    )
}

#[test]
fn claude_usage_sums_the_block_and_the_week_incrementally() {
    let root = scratch("claude-usage");
    transcripts(&root);
    let text = run(&usage_script(&root, ""), 10);
    // block 160 + 1000, week + 5000; the peak block is the 5000 on the 17th.
    assert!(
        text.starts_with(
            "1160 2 6160 3 5000 6160 2;1161 3 6161 4 5000 6161 2;1161 3 6161 4 5000 6161 2;done;"
        ),
        "{text}"
    );
    assert!(root.join("state.json").exists());
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn claude_usage_reads_large_files_in_chunks() {
    // Files over `max_read` go through `read_range`, here a stand-in for dd
    // that slices the file, in chunks smaller than a line.
    let root = scratch("claude-usage-chunks");
    transcripts(&root);
    let extra = r#", max_read = 64, chunk = 700,
        read_range = function(path, offset, length, on_done)
          local text = morf.fs.read(path)
          morf.timer(1, function() on_done(text:sub(offset + 1, offset + length)) end, false)
        end"#;
    let text = run(&usage_script(&root, extra), 20);
    assert!(
        text.starts_with(
            "1160 2 6160 3 5000 6160 2;1161 3 6161 4 5000 6161 2;1161 3 6161 4 5000 6161 2;done;"
        ),
        "{text}"
    );
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn claude_usage_asks_for_the_limits_only_when_called() {
    let root = scratch("claude-limits");
    put(
        &root,
        "/.credentials.json",
        r#"{"claudeAiOauth":{"accessToken":"tok"}}"#,
    );
    let (base, hits) = serve(vec![("/v1/messages", 200, "{}".to_owned())]);
    // The server answers without the headers: the limits are unavailable, and
    // the request carried the token and the smallest model.
    let text = run(
        &format!(
            r##"
            local claude_usage = require("lib.claude_usage")
            claude_usage.limits({{ credentials = "{root}/.credentials.json", url = "{base}/v1/messages" }},
              function(result)
                note(tostring(result.available) .. " " .. tostring(result.error ~= nil))
                claude_usage.limits({{ credentials = "{root}/missing.json" }}, function(missing)
                  note(tostring(missing.available))
                  note("done")
                end)
              end)
            "##,
            root = root.display()
        ),
        10,
    );
    assert!(text.contains("false true;false;done;"), "{text}");
    let hits = hits.lock().unwrap().clone();
    assert_eq!(hits.len(), 1);
    assert!(hits[0].contains("\"max_tokens\":1"), "{hits:?}");
    std::fs::remove_dir_all(&root).unwrap();
}

// ---------------------------------------------------------------------------
// sysinfo

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
            local sysinfo = require("lib.sysinfo")
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
            local sysinfo = require("lib.sysinfo")
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
        local sysinfo = require("lib.sysinfo")
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
