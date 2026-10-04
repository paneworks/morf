//! weather: Open-Meteo with a geocoded place and a cache, the wttr.in
//! fallback, and the words, icons and glyphs for weather codes.

use super::*;

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
            local weather = require("lib.integrations.weather")
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
            local weather = require("lib.integrations.weather")
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
        local weather = require("lib.integrations.weather")
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
