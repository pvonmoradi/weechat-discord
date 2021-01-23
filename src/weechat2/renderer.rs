use crate::refcell::RefCell;
use std::{collections::VecDeque, marker::PhantomData, rc::Rc};
use weechat::buffer::BufferHandle;

pub trait WeechatMessage<I, S> {
    /// Format the message into the prefix and body
    fn render(&self, state: &mut S) -> (String, String);
    fn tags(&self, state: &mut S, notify: bool) -> Vec<&'static str>;
    fn timestamp(&self, state: &mut S) -> i64;
    fn id(&self, state: &mut S) -> I;
}

pub struct MessageRenderer<M: WeechatMessage<I, S> + Clone, I: Eq, S> {
    messages: Rc<RefCell<VecDeque<M>>>,
    buffer_handle: Rc<BufferHandle>,
    state: Rc<RefCell<S>>,
    max_buffer_messages: Rc<usize>,
    phantom_id: PhantomData<I>,
}

impl<M: WeechatMessage<I, S> + Clone, I: Eq, S> Clone for MessageRenderer<M, I, S> {
    fn clone(&self) -> Self {
        Self {
            messages: Rc::clone(&self.messages),
            buffer_handle: Rc::clone(&self.buffer_handle),
            state: Rc::clone(&self.state),
            max_buffer_messages: Rc::clone(&self.max_buffer_messages),
            phantom_id: self.phantom_id,
        }
    }
}

impl<M: WeechatMessage<I, S> + Clone, I: Eq, S> MessageRenderer<M, I, S> {
    pub fn new(buffer_handle: Rc<BufferHandle>, max_buffer_messages: usize, state: S) -> Self {
        Self {
            buffer_handle,
            state: Rc::new(RefCell::new(state)),
            max_buffer_messages: Rc::new(max_buffer_messages),
            messages: Rc::new(RefCell::new(VecDeque::new())),
            phantom_id: PhantomData,
        }
    }

    pub fn buffer_handle(&self) -> Rc<BufferHandle> {
        self.buffer_handle.clone()
    }

    pub fn messages(&self) -> Rc<RefCell<VecDeque<M>>> {
        self.messages.clone()
    }

    pub fn state(&self) -> Rc<RefCell<S>> {
        self.state.clone()
    }

    fn print_msg(&self, msg: &M, notify: bool, log: bool) {
        let buffer = self
            .buffer_handle
            .upgrade()
            .expect("message renderer outlived buffer");

        let mut state = self.state.borrow_mut();
        let (prefix, suffix) = msg.render(&mut state);
        let mut tags = msg.tags(&mut state, notify);
        if !log {
            tags.push("no_log");
        }
        buffer.print_date_tags(
            msg.timestamp(&mut state),
            &tags,
            &format!("{}\t{}", prefix, suffix),
        );
    }

    pub fn redraw_buffer(&self) {
        self.buffer_handle
            .upgrade()
            .expect("message renderer outlived buffer")
            .clear();

        for message in self.messages.borrow().iter().rev() {
            self.print_msg(&message, false, false);
        }
    }

    pub fn add_msg(&self, msg: M, notify: bool) {
        self.print_msg(&msg, notify, true);

        let mut messages = self.messages.borrow_mut();
        messages.push_front(msg);
        messages.truncate(*self.max_buffer_messages);
    }

    pub fn add_bulk_msgs(&self, msgs: impl DoubleEndedIterator<Item = M>) {
        let mut messages = self.messages.borrow_mut();
        messages.extend(msgs.rev().take(*self.max_buffer_messages));
        messages.truncate(*self.max_buffer_messages);
        for msg in messages.iter().rev() {
            self.print_msg(msg, false, false);
        }
    }

    pub fn update_message<F>(&self, id: I, f: F)
    where
        F: FnOnce(&mut M),
    {
        let mut state = self.state.borrow_mut();
        if let Some(msg) = self
            .messages
            .borrow_mut()
            .iter_mut()
            .find(|msg| msg.id(&mut state) == id)
        {
            f(msg)
        }
    }

    pub fn get_nth_message(&self, index: usize) -> Option<M> {
        self.messages.borrow().iter().nth(index).cloned()
    }

    pub fn remove_msg(&self, id: I) {
        {
            let mut state = self.state.borrow_mut();
            let index = self
                .messages
                .borrow()
                .iter()
                .position(|it| it.id(&mut state) == id);
            if let Some(index) = index {
                self.messages.borrow_mut().remove(index);
            }
        }
        self.redraw_buffer();
    }

    pub fn remove(&self, index: usize) -> Option<M> {
        self.messages.borrow_mut().remove(index)
    }
}
