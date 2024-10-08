//! Some FFI examples
//!
//! Adapted from the system-configuration-sys crate

use core::ffi::c_void;
use core_foundation_sys::array::CFArrayRef;
use core_foundation_sys::base::{Boolean, CFAllocatorRef, CFIndex, CFTypeID};
use core_foundation_sys::dictionary::CFDictionaryRef;
use core_foundation_sys::propertylist::CFPropertyListRef;
use core_foundation_sys::runloop::CFRunLoopSourceRef;
use core_foundation_sys::string::CFStringRef;

use crate::dispatch_queue_t;

#[repr(C)]
pub struct __SCDynamicStore(c_void);

pub type SCDynamicStoreRef = *const __SCDynamicStore;
#[repr(C)]
pub struct SCDynamicStoreContext {
    pub version: CFIndex,
    pub info: *mut ::core::ffi::c_void,
    pub retain: Option<
        unsafe extern "C" fn(info: *const ::core::ffi::c_void) -> *const ::core::ffi::c_void,
    >,
    pub release: Option<unsafe extern "C" fn(info: *const ::core::ffi::c_void)>,
    pub copyDescription:
        Option<unsafe extern "C" fn(info: *const ::core::ffi::c_void) -> CFStringRef>,
}
pub type SCDynamicStoreCallBack = Option<
    unsafe extern "C" fn(
        store: SCDynamicStoreRef,
        changedKeys: CFArrayRef,
        info: *mut ::core::ffi::c_void,
    ),
>;
extern "C" {
    pub fn SCDynamicStoreGetTypeID() -> CFTypeID;

    pub fn SCDynamicStoreCreate(
        allocator: CFAllocatorRef,
        name: CFStringRef,
        callout: SCDynamicStoreCallBack,
        context: *mut SCDynamicStoreContext,
    ) -> SCDynamicStoreRef;

    pub fn SCDynamicStoreCreateWithOptions(
        allocator: CFAllocatorRef,
        name: CFStringRef,
        storeOptions: CFDictionaryRef,
        callout: SCDynamicStoreCallBack,
        context: *mut SCDynamicStoreContext,
    ) -> SCDynamicStoreRef;

    pub static kSCDynamicStoreUseSessionKeys: CFStringRef;

    pub fn SCDynamicStoreCreateRunLoopSource(
        allocator: CFAllocatorRef,
        store: SCDynamicStoreRef,
        order: CFIndex,
    ) -> CFRunLoopSourceRef;

    pub fn SCDynamicStoreSetDispatchQueue(
        store: SCDynamicStoreRef,
        queue: dispatch_queue_t,
    ) -> Boolean;

    pub fn SCDynamicStoreCopyKeyList(store: SCDynamicStoreRef, pattern: CFStringRef) -> CFArrayRef;

    pub fn SCDynamicStoreAddValue(
        store: SCDynamicStoreRef,
        key: CFStringRef,
        value: CFPropertyListRef,
    ) -> Boolean;

    pub fn SCDynamicStoreAddTemporaryValue(
        store: SCDynamicStoreRef,
        key: CFStringRef,
        value: CFPropertyListRef,
    ) -> Boolean;

    pub fn SCDynamicStoreCopyValue(store: SCDynamicStoreRef, key: CFStringRef)
        -> CFPropertyListRef;

    pub fn SCDynamicStoreCopyMultiple(
        store: SCDynamicStoreRef,
        keys: CFArrayRef,
        patterns: CFArrayRef,
    ) -> CFDictionaryRef;

    pub fn SCDynamicStoreSetValue(
        store: SCDynamicStoreRef,
        key: CFStringRef,
        value: CFPropertyListRef,
    ) -> Boolean;

    pub fn SCDynamicStoreSetMultiple(
        store: SCDynamicStoreRef,
        keysToSet: CFDictionaryRef,
        keysToRemove: CFArrayRef,
        keysToNotify: CFArrayRef,
    ) -> Boolean;

    pub fn SCDynamicStoreRemoveValue(store: SCDynamicStoreRef, key: CFStringRef) -> Boolean;

    pub fn SCDynamicStoreNotifyValue(store: SCDynamicStoreRef, key: CFStringRef) -> Boolean;

    pub fn SCDynamicStoreSetNotificationKeys(
        store: SCDynamicStoreRef,
        keys: CFArrayRef,
        patterns: CFArrayRef,
    ) -> Boolean;

    pub fn SCDynamicStoreCopyNotifiedKeys(store: SCDynamicStoreRef) -> CFArrayRef;
}

fn main() {
    println!("Hello, world!");
}
