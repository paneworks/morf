//! github: the contributions page, and GraphQL when there is a token.

use super::*;

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
            local github = require("lib.integrations.github")
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
            local github = require("lib.integrations.github")
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
