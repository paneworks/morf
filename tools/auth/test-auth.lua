local sessions, opened, failures, busy = {}, 0, 0, false
package.preload.morf = function() return {
  env=function() return '/never-connect-this-mock' end,
  greetd={converse=function(user)
    local login={user=user,answers={},cancelled=0,starts=0}
    function login:on_message(fn) self.emit=fn end
    function login:respond(answer) self.answers[#self.answers+1]=answer or '<ack>' end
    function login:cancel() self.cancelled=self.cancelled+1 end
    function login:start() self.starts=self.starts+1 end
    sessions[#sessions+1]=login
    return login
  end},
} end
local auth=dofile('library/lib/auth.lua')
local function door(opts)
  opts=opts or {}
  opts.user='test-user';opts.session={command={'true'},environment={}}
  opts.on_busy=function(value) busy=value end
  opts.on_failed=function() failures=failures+1 end
  opts.on_open=function() opened=opened+1 end
  return auth.greeter(opts)
end
local d=door()
assert(#sessions==0,'startup must not authenticate')
d:switch('another-user'); assert(#sessions==0,'account selection must not authenticate')
d:submit('');d:submit(nil);assert(#sessions==0,'empty submission must not authenticate')
d:submit('test-answer');assert(#sessions==1 and busy)
d:submit('duplicate');assert(#sessions==1,'duplicate submit')
local first=sessions[1]
first.emit{kind='auth',auth_type='secret'};assert(first.answers[1]=='test-answer')
first.emit{kind='error',text='Account locked'}
assert(#sessions==1 and failures==1 and not busy,'error must not retry')
assert(d.held==nil and d.login==nil,'clear held secret')
first.emit{kind='success'};assert(opened==0 and first.starts==0,'stale callback')
d:submit('next-answer');assert(#sessions==2,'explicit retry allowed')
local second=sessions[2]
second.emit{kind='auth',auth_type='info',text='Please wait'}
second.emit{kind='auth',auth_type='secret'}
assert(second.answers[1]=='<ack>' and second.answers[2]=='next-answer')
second.emit{kind='success'};assert(second.starts==1)
second.emit{kind='success'};assert(opened==1)
local preview=door{enabled=false};preview:submit('answer')
assert(#sessions==2,'preview must never connect even with GREETD_SOCK')
local stopped=door();stopped:submit('answer');local third=sessions[3]
stopped:switch('other');third.emit{kind='success'}
assert(#sessions==3 and third.starts==0 and not busy,'switch must cancel without retry')
print('PASS: startup, account switching, empty/duplicate submit, failure, explicit retry, stale callbacks, successful login, preview isolation')

local disconnected=door();disconnected:submit('answer')
local last=sessions[#sessions];local count=#sessions
last.emit{kind='failed',text='connection lost'}
assert(not busy and disconnected.held==nil and disconnected.login==nil)
assert(#sessions==count,'transport error must stop without retrying')
print('PASS: transport failure releases busy state without retrying')
