use rouille::{router, Request, Response};
use rpi_messages_common::UpdateID;
use std::sync::Mutex;

use crate::message::{Message, MessageContent, Messages, SenderID};
use crate::{Result, MESSAGES};

pub fn run() {
    rouille::start_server("0.0.0.0:3000", move |request| {
        let result: Result<Response> = (|| {
            router!(request,
                (GET) ["/new/{msg}", msg: String] => {
                    let mut guard = MESSAGES.lock().unwrap();
                    let new_message_content: MessageContent = MessageContent::new_text(msg)?;
                    let new_message = Message::new(guard.next_id(), 0xcafebabe, SenderID::Web, chrono::Utc::now().naive_utc(), chrono::Duration::hours(24), new_message_content);

                    guard.add_message(new_message);
                    Ok(Response::text("added mssage"))
                },
                (GET) ["/latest/{after}", after: UpdateID] => {
                    let guard = MESSAGES.lock().unwrap();
                    match guard.get_next_message(0xcafebabe, Some(after)) {
                        Some(Message{content: MessageContent::Text(text), ..}) => {
                            Ok(Response::text(text))
                        }
                        _ => Ok(Response::empty_404())
                    }
                },
                _ => {
                    Ok(Response::empty_404())
                }
            )
        })();

        match result {
            Ok(response) => response,
            Err(e) => {
                eprintln!("{}", e);
                Response::empty_400()
            }
        }
    });
}
