//! poll: programs run directly and jobs run in slices.

use super::*;

#[test]
fn poll_runs_programs_directly_and_jobs_in_slices() {
    let text = run(
        r##"
        local poll = require("lib.util.poll")
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
