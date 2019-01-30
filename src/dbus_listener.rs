// flatkvm
// Copyright (C) 2019  Sergio Lopez <slp@sinrega.org>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <http://www.gnu.org/licenses/>.

use crate::message::Message;
use dbus::{BusType, Connection, ConnectionItem, SignalArgs};
use flatkvm_qemu::dbus_codegen::*;
use flatkvm_qemu::dbus_notifications::DbusNotificationClosed;
use std::sync::mpsc::Sender;

pub fn handle_dbus_notifications(sender: Sender<Message>) {
    let c = Connection::get_private(BusType::Session).unwrap();
    // Add a match for this signal
    let mstr = OrgFreedesktopNotificationsNotificationClosed::match_str(None, None);
    c.add_match(&mstr).unwrap();

    // Wait for the signal to arrive.
    loop {
        match c.iter(-1).next() {
            Some(ConnectionItem::Signal(msg)) => {
                if let Some(nc) = OrgFreedesktopNotificationsNotificationClosed::from_message(&msg)
                {
                    sender
                        .send(Message::DbusNotificationClosed(DbusNotificationClosed {
                            id: nc.id,
                            reason: nc.reason,
                        }))
                        .unwrap();
                }
            }
            _ => (),
        }
    }
}
