//! Messages at the window's foot. Information goes away by itself after a few seconds; an error
//! stays until dismissed, as it may need reading twice, and no more than four stay at once.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use openrac_launcher_service::{hide_paths, Notice, NoticeKind};
use slint::{ComponentHandle, ModelRc, Timer, VecModel};

use crate::{App, AppWindow, Toast};

const INFO_FOR: Duration = Duration::from_secs(4);
const MOST_ERRORS: usize = 4;

#[derive(Default)]
pub struct Toasts {
    shown: RefCell<Vec<Toast>>,
    next: RefCell<i32>,
}

impl Toasts {
    /// Shows `notice`, its folders cut to their names (players share screenshots).
    pub fn show(self: &Rc<Self>, window: &AppWindow, notice: Notice) {
        let id = {
            let mut next = self.next.borrow_mut();
            *next += 1;
            *next
        };
        let error = notice.kind == NoticeKind::Error;
        let toast = Toast {
            id,
            text: hide_paths(&notice.text).0.into(),
            details: notice.details.map(|d| hide_paths(&d).0).unwrap_or_default().into(),
            error,
        };
        self.shown.borrow_mut().push(toast);
        let oldest_error = {
            let shown = self.shown.borrow();
            let errors: Vec<i32> = shown.iter().filter(|t| t.error).map(|t| t.id).collect();
            (errors.len() > MOST_ERRORS).then(|| errors[0])
        };
        if let Some(oldest) = oldest_error {
            self.dismiss(window, oldest);
        }
        if !error {
            let toasts = Rc::downgrade(self);
            let window = window.as_weak();
            Timer::single_shot(INFO_FOR, move || {
                if let (Some(toasts), Some(window)) = (toasts.upgrade(), window.upgrade()) {
                    toasts.dismiss(&window, id);
                }
            });
        }
        self.publish(window);
    }

    pub fn dismiss(&self, window: &AppWindow, id: i32) {
        self.shown.borrow_mut().retain(|t| t.id != id);
        self.publish(window);
    }

    fn publish(&self, window: &AppWindow) {
        let shown = self.shown.borrow().clone();
        window.global::<App>().set_toasts(ModelRc::new(VecModel::from(shown)));
    }
}
