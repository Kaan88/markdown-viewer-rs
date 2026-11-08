//! Keyboard input through XAML's own routed events: `UIElement.KeyDown`, `CharacterReceived`,
//! `IsTabStop` and `Focus`. The generated `bindings.rs` is trimmed with `minimal()` and leaves these
//! as unbound slots (or cuts the vtable off before them), so this module calls them by slot index.
//!
//! The indices and IIDs come from `Microsoft.UI.Xaml.winmd` in the Windows App SDK 2.6.4 NuGet
//! package (`ildasm` output, method order per interface), and the `IUIElement` slot names before
//! `PointerWheelChanged` were checked one for one against the generated `IUIElement_Vtbl`.
//!
//! Using routed events instead of a thread message hook means a key reaches the page only when no
//! control handled it first, so the menu bar keeps its own arrow, Space and Escape navigation.
use crate::bindings::{TypedEventHandler, UIElement};
use windows_core::{GUID, HRESULT, IInspectable, IUnknown, Interface, Ref, RuntimeType};

/// `IInspectable` has six slots (three `IUnknown`, three `IInspectable`) before an interface's own.
const BASE: usize = 6;
/// `IUIElement` method indices, counted from its first method.
const SET_IS_TAB_STOP: usize = 121;
const ADD_KEY_DOWN: usize = 126;
const REMOVE_KEY_DOWN: usize = 127;
const ADD_CHARACTER_RECEIVED: usize = 136;
const REMOVE_CHARACTER_RECEIVED: usize = 137;
const FOCUS: usize = 220;
/// `Microsoft.UI.Xaml.FocusState::Programmatic`.
pub const FOCUS_PROGRAMMATIC: i32 = 3;

/// The function pointer in slot `index` of `object`'s vtable.
///
/// # Safety
/// `object` must implement an interface with at least `index + 1` slots.
unsafe fn slot(object: &impl Interface, index: usize) -> usize {
    unsafe {
        let raw = object.as_raw() as *const *const usize;
        *(*raw).add(index)
    }
}

windows_core::imp::define_interface!(
    IKeyRoutedEventArgs,
    IKeyRoutedEventArgs_Vtbl,
    0xee357007_a2d6_5c75_9431_05fd66ec7915
);
impl RuntimeType for IKeyRoutedEventArgs {
    const SIGNATURE: windows_core::imp::ConstBuffer =
        windows_core::imp::ConstBuffer::for_interface::<Self>();
}
#[repr(C)]
pub struct IKeyRoutedEventArgs_Vtbl {
    base__: windows_core::IInspectable_Vtbl,
    key: unsafe extern "system" fn(*mut core::ffi::c_void, *mut i32) -> HRESULT,
    key_status: usize,
    handled: usize,
    set_handled: unsafe extern "system" fn(*mut core::ffi::c_void, bool) -> HRESULT,
}
impl IKeyRoutedEventArgs {
    /// The `Windows.System.VirtualKey`, which has the same values as Win32's `VK_*`.
    pub fn key(&self) -> windows_core::Result<u32> {
        let mut key = 0;
        unsafe { (self.vtable().key)(self.as_raw(), &mut key).ok()? };
        Ok(key as u32)
    }
    pub fn set_handled(&self, handled: bool) -> windows_core::Result<()> {
        unsafe { (self.vtable().set_handled)(self.as_raw(), handled).ok() }
    }
}

windows_core::imp::define_interface!(
    ICharacterReceivedRoutedEventArgs,
    ICharacterReceivedRoutedEventArgs_Vtbl,
    0xe26ca5bb_34c3_5c1e_9a16_00b80b07a899
);
impl RuntimeType for ICharacterReceivedRoutedEventArgs {
    const SIGNATURE: windows_core::imp::ConstBuffer =
        windows_core::imp::ConstBuffer::for_interface::<Self>();
}
#[repr(C)]
pub struct ICharacterReceivedRoutedEventArgs_Vtbl {
    base__: windows_core::IInspectable_Vtbl,
    character: unsafe extern "system" fn(*mut core::ffi::c_void, *mut u16) -> HRESULT,
    key_status: usize,
    handled: usize,
    set_handled: unsafe extern "system" fn(*mut core::ffi::c_void, bool) -> HRESULT,
}
impl ICharacterReceivedRoutedEventArgs {
    /// One UTF-16 unit; a character outside the BMP arrives as two events.
    pub fn character(&self) -> windows_core::Result<u16> {
        let mut unit = 0;
        unsafe { (self.vtable().character)(self.as_raw(), &mut unit).ok()? };
        Ok(unit)
    }
    pub fn set_handled(&self, handled: bool) -> windows_core::Result<()> {
        unsafe { (self.vtable().set_handled)(self.as_raw(), handled).ok() }
    }
}

/// The runtime class, needed only for the `TypedEventHandler<UIElement, _>` signature, whose IID
/// is derived from the class name and its default interface.
#[repr(transparent)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CharacterReceivedRoutedEventArgs(IUnknown);
windows_core::imp::interface_hierarchy!(CharacterReceivedRoutedEventArgs, IUnknown, IInspectable);
impl RuntimeType for CharacterReceivedRoutedEventArgs {
    const SIGNATURE: windows_core::imp::ConstBuffer =
        windows_core::imp::ConstBuffer::for_class::<Self, ICharacterReceivedRoutedEventArgs>();
}
unsafe impl Interface for CharacterReceivedRoutedEventArgs {
    type Vtable = ICharacterReceivedRoutedEventArgs_Vtbl;
    const IID: GUID = <ICharacterReceivedRoutedEventArgs as Interface>::IID;
}
impl core::ops::Deref for CharacterReceivedRoutedEventArgs {
    type Target = ICharacterReceivedRoutedEventArgs;
    fn deref(&self) -> &Self::Target {
        unsafe { core::mem::transmute(self) }
    }
}
impl windows_core::RuntimeName for CharacterReceivedRoutedEventArgs {
    const NAME: &'static str = "Microsoft.UI.Xaml.Input.CharacterReceivedRoutedEventArgs";
}

windows_core::imp::define_interface!(
    KeyEventHandler,
    KeyEventHandler_Vtbl,
    0xdb68e7cc_9a2b_527d_9989_25284daccc03
);
impl RuntimeType for KeyEventHandler {
    const SIGNATURE: windows_core::imp::ConstBuffer =
        windows_core::imp::ConstBuffer::for_interface::<Self>();
}
#[repr(C)]
pub struct KeyEventHandler_Vtbl {
    base__: windows_core::IUnknown_Vtbl,
    invoke: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
    ) -> HRESULT,
}
struct KeyEventHandlerBox<F: Fn(Ref<IInspectable>, Ref<IKeyRoutedEventArgs>) + 'static>(
    core::marker::PhantomData<(fn() -> F,)>,
);
impl<F: Fn(Ref<IInspectable>, Ref<IKeyRoutedEventArgs>) + 'static> KeyEventHandlerBox<F> {
    const VTABLE: KeyEventHandler_Vtbl = KeyEventHandler_Vtbl {
        base__: windows_core::IUnknown_Vtbl {
            QueryInterface: windows_core::imp::DelegateBox::<KeyEventHandler, F>::QueryInterface,
            AddRef: windows_core::imp::DelegateBox::<KeyEventHandler, F>::AddRef,
            Release: windows_core::imp::DelegateBox::<KeyEventHandler, F>::Release,
        },
        invoke: Self::invoke,
    };
    unsafe extern "system" fn invoke(
        this: *mut core::ffi::c_void,
        sender: *mut core::ffi::c_void,
        e: *mut core::ffi::c_void,
    ) -> HRESULT {
        unsafe {
            let this = &mut *(this as *mut *mut core::ffi::c_void
                as *mut windows_core::imp::DelegateBox<KeyEventHandler, F>);
            (this.invoke)(
                core::mem::transmute_copy(&sender),
                core::mem::transmute_copy(&e),
            );
            HRESULT(0)
        }
    }
}

/// Calls an `add_X(handler, *token)` slot and returns a revoker for its `remove_X`.
fn register(
    element: &UIElement,
    add: usize,
    remove: usize,
    handler: &impl Interface,
) -> windows_core::Result<windows_core::EventRevoker> {
    unsafe {
        let add: unsafe extern "system" fn(
            *mut core::ffi::c_void,
            *mut core::ffi::c_void,
            *mut i64,
        ) -> HRESULT = core::mem::transmute(slot(element, BASE + add));
        let mut token = 0;
        add(element.as_raw(), handler.as_raw(), &mut token).ok()?;
        let remove: unsafe extern "system" fn(*mut core::ffi::c_void, i64) -> HRESULT =
            core::mem::transmute(slot(element, BASE + remove));
        Ok(windows_core::EventRevoker::new(
            element.clone(),
            token,
            remove,
        ))
    }
}

/// `UIElement.KeyDown`. It bubbles, so a handler on the root sees every key no control handled.
pub fn key_down<F>(
    element: &UIElement,
    handler: F,
) -> windows_core::Result<windows_core::EventRevoker>
where
    F: Fn(Ref<IInspectable>, Ref<IKeyRoutedEventArgs>) + 'static,
{
    let handler: KeyEventHandler = {
        let com = windows_core::imp::DelegateBox::<KeyEventHandler, F>::new(
            &KeyEventHandlerBox::<F>::VTABLE,
            handler,
        );
        unsafe { core::mem::transmute(windows_core::imp::box_new(com)) }
    };
    register(element, ADD_KEY_DOWN, REMOVE_KEY_DOWN, &handler)
}

/// `UIElement.CharacterReceived`: text after keyboard layout and dead keys, for the find query.
pub fn character_received<F>(
    element: &UIElement,
    handler: F,
) -> windows_core::Result<windows_core::EventRevoker>
where
    F: Fn(Ref<UIElement>, Ref<CharacterReceivedRoutedEventArgs>) + 'static,
{
    let handler: TypedEventHandler<UIElement, CharacterReceivedRoutedEventArgs> = {
        let com =
            windows_core::imp::DelegateBox::<
                TypedEventHandler<UIElement, CharacterReceivedRoutedEventArgs>,
                F,
            >::new(
                &crate::bindings::TypedEventHandlerBox::<
                    UIElement,
                    CharacterReceivedRoutedEventArgs,
                    F,
                >::VTABLE,
                handler,
            );
        unsafe { core::mem::transmute(windows_core::imp::box_new(com)) }
    };
    register(
        element,
        ADD_CHARACTER_RECEIVED,
        REMOVE_CHARACTER_RECEIVED,
        &handler,
    )
}

/// `UIElement.IsTabStop`: lets an element that is not a `Control` hold keyboard focus.
pub fn set_is_tab_stop(element: &UIElement, value: bool) -> windows_core::Result<()> {
    unsafe {
        let set: unsafe extern "system" fn(*mut core::ffi::c_void, bool) -> HRESULT =
            core::mem::transmute(slot(element, BASE + SET_IS_TAB_STOP));
        set(element.as_raw(), value).ok()
    }
}

/// `UIElement.Focus(FocusState)`; false when the element cannot take focus.
pub fn focus(element: &UIElement, state: i32) -> windows_core::Result<bool> {
    unsafe {
        let call: unsafe extern "system" fn(*mut core::ffi::c_void, i32, *mut bool) -> HRESULT =
            core::mem::transmute(slot(element, BASE + FOCUS));
        let mut moved = false;
        call(element.as_raw(), state, &mut moved).ok()?;
        Ok(moved)
    }
}
