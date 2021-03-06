use actix_router::Path;
use amqp_codec::{protocol::Attach, types::ByteStr};
use futures::{Async, Poll, Stream};

use crate::cell::Cell;
use crate::errors::AmqpTransportError;
use crate::rcvlink::ReceiverLink;

use super::errors::LinkError;
use super::proto::{Flow, Frame, Message};

pub struct OpenLink<S> {
    pub(crate) state: Cell<S>,
    pub(crate) link: ReceiverLink,
    pub(crate) path: Path<ByteStr>,
}

impl<S> OpenLink<S> {
    pub(crate) fn new(link: ReceiverLink, state: Cell<S>) -> Self {
        OpenLink {
            state,
            link,
            path: Path::new(ByteStr::from_str("")),
        }
    }

    pub fn path(&self) -> &Path<ByteStr> {
        &self.path
    }

    pub fn path_mut(&mut self) -> &mut Path<ByteStr> {
        &mut self.path
    }

    pub fn frame(&self) -> &Attach {
        self.link.frame()
    }

    pub fn state(&self) -> &S {
        self.state.get_ref()
    }

    pub fn state_mut(&mut self) -> &mut S {
        self.state.get_mut()
    }

    pub fn open(mut self, credit: u32) -> Link<S> {
        self.link.open();
        self.link.set_link_credit(credit);

        Link {
            state: self.state,
            link: self.link,
            has_credit: credit != 0,
        }
    }
}

pub struct Link<S> {
    pub(crate) state: Cell<S>,
    pub(crate) link: ReceiverLink,
    has_credit: bool,
}

impl<S> Link<S> {
    pub fn state(&self) -> &S {
        self.state.get_ref()
    }

    pub fn state_mut(&mut self) -> &mut S {
        self.state.get_mut()
    }
}

impl<S> Stream for Link<S> {
    type Item = Frame<S>;
    type Error = AmqpTransportError;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        if !self.has_credit {
            self.has_credit = true;
            Ok(Async::Ready(Some(
                Flow::new(self.state.clone(), self.link.clone()).into(),
            )))
        } else {
            match self.link.poll()? {
                Async::Ready(Some(transfer)) => {
                    // #2.7.5 delivery_id MUST be set. batching is not supported atm
                    if transfer.delivery_id.is_none() {
                        self.link.close_with_error(
                            LinkError::force_detach()
                                .description("delivery_id MUST be set")
                                .into(),
                        );
                    }

                    self.has_credit = self.link.credit() != 0;
                    Ok(Async::Ready(Some(
                        Message::new(self.state.clone(), transfer, self.link.clone()).into(),
                    )))
                }
                Async::Ready(None) => Ok(Async::Ready(None)),
                Async::NotReady => Ok(Async::NotReady),
            }
        }
    }
}
