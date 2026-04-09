use std::cell::RefCell;

thread_local! {
    static CURRENT_APP_ID: RefCell<String> = const { RefCell::new(String::new()) };
}

pub(crate) fn set_current_app_id(id: &str) {
    CURRENT_APP_ID.with(|slot| {
        let app_id = id.split('/').next().unwrap_or(id).to_string();
        *slot.borrow_mut() = app_id;
    });
}

pub(crate) fn current_app_id() -> String {
    CURRENT_APP_ID.with(|slot| slot.borrow().clone())
}
