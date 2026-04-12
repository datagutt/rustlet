use std::cell::RefCell;

use tink_core::HybridDecrypt;

thread_local! {
    static CURRENT_APP_ID: RefCell<String> = const { RefCell::new(String::new()) };
    static SECRET_DECRYPTER: RefCell<Option<Box<dyn HybridDecrypt>>> = const { RefCell::new(None) };
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

pub(crate) fn set_secret_decrypter(decrypter: Option<Box<dyn HybridDecrypt>>) {
    SECRET_DECRYPTER.with(|slot| {
        *slot.borrow_mut() = decrypter;
    });
}

pub(crate) fn with_secret_decrypter<R>(
    f: impl FnOnce(Option<&dyn HybridDecrypt>) -> anyhow::Result<R>,
) -> anyhow::Result<R> {
    SECRET_DECRYPTER.with(|slot| {
        let borrowed = slot.borrow();
        f(borrowed.as_deref())
    })
}
