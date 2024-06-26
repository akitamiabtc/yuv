use std::sync::Arc;

pub fn mut_mock<T>(mock: &mut Arc<T>) -> &mut T {
    let ptr: *mut T = Arc::as_ptr(mock) as *mut T;
    unsafe { ptr.as_mut().unwrap() }
}
