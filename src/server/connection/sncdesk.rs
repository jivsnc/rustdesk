//! SNCDesk: one approval per specialist (custom/README.md).
//!
//! With `approve-mode = click` every connection asks the operator, so a specialist who
//! opens file transfer next to remote control is asked about twice. While the peer still
//! has a connection the operator accepted, its next one is let in without asking; the
//! connection manager shows it like any other, so the operator can still close it.

use super::*;

/// Whether `peer_id` has an authorized remote control or file transfer connection now.
pub(super) fn is_peer_accepted(peer_id: &str) -> bool {
    !peer_id.is_empty()
        && AUTHED_CONNS.lock().unwrap().iter().any(|c| {
            c.session_key.peer_id == peer_id
                && matches!(c.conn_type, AuthConnType::Remote | AuthConnType::FileTransfer)
        })
}
