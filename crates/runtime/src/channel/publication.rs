use super::*;

struct Inspection<'a, T> {
    identity: &'a Rc<()>,
    inspect: &'a mut PublicationInspection<'a, T>,
    channels: &'a [Channel<T>],
    work: &'a e::ServiceWork,
    visited: &'a mut [bool],
    stack: &'a mut Vec<usize>,
    target: usize,
    cycle: bool,
}
impl<T: Payload> Inspection<'_, T> {
    fn value(&mut self, value: PublicationRoot<'_, T>) -> Result<(), Error> {
        if matches!(&value, PublicationRoot::Message(_)) {
            self.work.charge(1)?;
        }
        let identity = self.identity;
        let channels = self.channels;
        let visited = &mut self.visited;
        let stack = &mut self.stack;
        let target = self.target;
        let cycle = &mut self.cycle;
        (self.inspect)(value, &mut |receiver| {
            if *cycle {
                return Ok(());
            }
            self.work.charge(1)?;
            let key = &receiver.0;
            if !Rc::ptr_eq(identity, &key.identity) {
                return Err(Error::ForeignEndpoint);
            }
            let channel = channels.get(key.index).ok_or(Error::InvalidEndpoint)?;
            if channel.role != Role::Receiver {
                return Err(Error::WrongRole);
            }
            if key.index == target {
                *cycle = true;
            } else if !visited[key.index] {
                visited[key.index] = true;
                stack.push(key.index);
            }
            Ok(())
        })
    }

    fn check(mut self, value: PublicationRoot<'_, T>) -> Result<bool, Error> {
        self.visited.fill(false);
        self.stack.clear();
        self.value(value)?;
        let channels = self.channels;
        while !self.cycle {
            let Some(index) = self.stack.pop() else { break };
            for entry in channels[index].values.entries() {
                self.value(PublicationRoot::Message(&entry.value.value))?;
                if self.cycle {
                    break;
                }
            }
            if !self.cycle {
                self.value(PublicationRoot::State(index))?;
            }
        }
        Ok(!self.cycle)
    }
}

impl<T: Payload> Hub<T> {
    pub(super) fn publication(
        &mut self,
        target: usize,
        value: &T,
        inspect: &mut PublicationInspection<'_, T>,
    ) -> Result<bool, Error> {
        Inspection {
            identity: &self.identity,
            inspect,
            channels: &self.channels,
            work: &self.work,
            visited: &mut self.visited,
            stack: &mut self.inspection,
            target,
            cycle: false,
        }
        .check(PublicationRoot::Message(value))
    }

    /// Check the prospective state of an existing or not-yet-published channel.
    /// The read-only adapter substitutes only this root; all reached messages
    /// and other states are read from their actual committed storage.
    pub(crate) fn state_publication(
        &mut self,
        target: usize,
        inspect: &mut PublicationInspection<'_, T>,
    ) -> Result<bool, Error> {
        if target > self.channels.len() {
            return Err(Error::InvalidEndpoint);
        }
        Inspection {
            identity: &self.identity,
            inspect,
            channels: &self.channels,
            work: &self.work,
            visited: &mut self.visited,
            stack: &mut self.inspection,
            target,
            cycle: false,
        }
        .check(PublicationRoot::State(target))
    }

    pub(super) fn pending_publication(
        &mut self,
        slot: usize,
        inspect: &mut PublicationInspection<'_, T>,
    ) -> Result<bool, Error> {
        let pending = self.pending[slot].as_ref().unwrap();
        let Kind::Send(message) = &pending.kind else {
            return Err(Error::WrongRole);
        };
        Inspection {
            identity: &self.identity,
            inspect,
            channels: &self.channels,
            work: &self.work,
            visited: &mut self.visited,
            stack: &mut self.inspection,
            target: pending.channel,
            cycle: false,
        }
        .check(PublicationRoot::Message(&message.value))
    }
}
