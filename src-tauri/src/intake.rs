//! Where messages from connected keyboards come in. A transport (USB in `hid`, Bluetooth in
//! `ble`) opens a [`Connection`] per keyboard and hands it the raw bytes it reads; the
//! connection parses them and tells the [`Sink`] (the `Hub` in the app). Dropping the
//! connection tells the sink the keyboard is gone.

use crate::hub::Transport;
use crate::protocol::{self, Message, Report};
use std::sync::Arc;

/// A connected keyboard as a transport sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    /// Unique while connected, e.g. `usb:<path>` or `ble:<id>`
    pub id: String,
    /// The USB product name or Bluetooth name
    pub name: String,
    pub transport: Transport,
}

/// What happens with the messages.
pub trait Sink: Send + Sync {
    /// The keyboard's layer and batteries, from the KeyLay firmware hook
    fn state(&self, from: &Source, report: Report);
    /// Battery levels alone, from the standard Battery Service
    fn batteries(&self, from: &Source, levels: Vec<Option<u8>>);
    /// The KeyLay key was pressed
    fn keylay_key(&self, from: &Source);
    /// The keyboard disconnected
    fn gone(&self, from: &Source);
}

pub struct Connection {
    sink: Arc<dyn Sink>,
    source: Source,
}

impl Connection {
    pub fn open(sink: Arc<dyn Sink>, id: String, name: String, transport: Transport) -> Connection {
        Connection { sink, source: Source { id, name, transport } }
    }

    /// A message read from the keyboard. Returns whether it was a state message, which tells
    /// the transport the keyboard has the KeyLay hook. Anything else is ignored.
    pub fn bytes(&self, buf: &[u8]) -> bool {
        match protocol::parse_message(buf) {
            Some(Message::State(report)) => {
                self.sink.state(&self.source, report);
                true
            }
            Some(Message::TogglePopover) => {
                self.sink.keylay_key(&self.source);
                false
            }
            None => false,
        }
    }

    pub fn batteries(&self, levels: Vec<Option<u8>>) {
        self.sink.batteries(&self.source, levels);
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        self.sink.gone(&self.source);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Default)]
    struct Record(Mutex<Vec<String>>);

    impl Sink for Record {
        fn state(&self, from: &Source, report: Report) {
            self.0.lock().unwrap().push(format!("{} state {}", from.id, report.layer));
        }
        fn batteries(&self, from: &Source, levels: Vec<Option<u8>>) {
            self.0.lock().unwrap().push(format!("{} batteries {levels:?}", from.id));
        }
        fn keylay_key(&self, from: &Source) {
            self.0.lock().unwrap().push(format!("{} key", from.id));
        }
        fn gone(&self, from: &Source) {
            self.0.lock().unwrap().push(format!("{} gone", from.id));
        }
    }

    #[test]
    fn routes_each_message_and_the_disconnect() {
        let record = Arc::new(Record::default());
        {
            let c = Connection::open(record.clone(), "usb:1".into(), "Charybdis".into(), Transport::Usb);
            assert!(c.bytes(&[0x4B, 1, 0x01, 2, 0b101, 0, 0, 0, 1, 80]));
            assert!(!c.bytes(&[0x4B, 1, 0x03, 0x01]));
            assert!(!c.bytes(&[0x00, 1, 2]));
            c.batteries(vec![Some(50)]);
        }
        assert_eq!(
            *record.0.lock().unwrap(),
            ["usb:1 state 2", "usb:1 key", "usb:1 batteries [Some(50)]", "usb:1 gone"]
        );
    }
}
