//! Logins under way: `morf.pam` checks, PAM conversations and greetd
//! sessions, each with who hears how it goes.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use morf_runtime::Handler;
use morf_system::{GreetdConversation, GreetdEvent, PamError, PamEvent, PamSession, PamTask};

use crate::runtime::pending::{PendingGreetdSession, PendingPam, PendingPamSession};

#[derive(Default)]
pub(crate) struct Auth {
    tasks: Vec<PendingPam>,
    sessions: Vec<PendingPamSession>,
    greetd: Vec<PendingGreetdSession>,
}

impl Auth {
    /// Waits on a PAM check for `callback`.
    pub(crate) fn check(&mut self, task: PamTask, callback: Handler, unlock_on_success: bool) {
        self.tasks.push(PendingPam {
            task,
            callback,
            unlock_on_success,
        });
    }

    /// The checks that have finished: who is told, whether a success
    /// unlocks, and the verdict.
    pub(crate) fn take_verdicts(&mut self) -> Vec<(Handler, bool, Result<(), PamError>)> {
        let mut ready = Vec::new();
        let mut index = 0;
        while index < self.tasks.len() {
            if let Some(result) = self.tasks[index].task.wait(Duration::ZERO) {
                let task = self.tasks.swap_remove(index);
                ready.push((task.callback, task.unlock_on_success, result));
            } else {
                index += 1;
            }
        }
        ready
    }

    /// How many PAM conversations have a listener.
    pub(crate) fn session_count(&self) -> usize {
        self.sessions.len()
    }

    /// Makes `callback` the one listener of `session`.
    pub(crate) fn listen_to_session(
        &mut self,
        session: &Rc<RefCell<PamSession>>,
        callback: Handler,
    ) {
        let entry = PendingPamSession {
            session: Rc::clone(session),
            callback,
        };
        match self
            .sessions
            .iter()
            .position(|entry| Rc::ptr_eq(&entry.session, session))
        {
            Some(index) => self.sessions[index] = entry,
            None => self.sessions.push(entry),
        }
    }

    /// What each conversation said this turn. A finished one leaves the
    /// list after its verdict is taken.
    pub(crate) fn take_session_messages(&mut self) -> Vec<(Handler, PamEvent)> {
        let mut messages = Vec::new();
        self.sessions.retain(|entry| {
            let mut finished = false;
            for _ in 0..8 {
                let Some(event) = entry.session.borrow_mut().next(Duration::ZERO) else {
                    break;
                };
                finished |= matches!(event, PamEvent::Finished(_));
                messages.push((entry.callback.clone(), event));
                if finished {
                    break;
                }
            }
            !finished
        });
        messages
    }

    /// Makes `callback` the one listener of a greetd `conversation`.
    pub(crate) fn listen_to_greetd(
        &mut self,
        conversation: &Rc<RefCell<GreetdConversation>>,
        callback: Handler,
    ) {
        let entry = PendingGreetdSession {
            conversation: Rc::clone(conversation),
            callback,
        };
        match self
            .greetd
            .iter()
            .position(|entry| Rc::ptr_eq(&entry.conversation, conversation))
        {
            Some(index) => self.greetd[index] = entry,
            None => self.greetd.push(entry),
        }
    }

    /// What greetd said this turn; an ended login leaves the list.
    pub(crate) fn take_greetd_messages(&mut self) -> Vec<(Handler, GreetdEvent)> {
        let mut messages = Vec::new();
        self.greetd.retain(|entry| {
            let mut conversation = entry.conversation.borrow_mut();
            for _ in 0..8 {
                let Some(event) = conversation.next(Duration::ZERO) else {
                    break;
                };
                let last = matches!(event, GreetdEvent::Failed(_));
                messages.push((entry.callback.clone(), event));
                if last {
                    break;
                }
            }
            !conversation.ended()
        });
        messages
    }
}
