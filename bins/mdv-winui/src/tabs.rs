//! `TabView`, `ToolTipService` and a few `Control`/`UIElement` setters the trimmed `bindings.rs`
//! leaves as unbound slots. Called by vtable slot index, as `input.rs` does.
//!
//! Indices and IIDs are from `ildasm` on `Microsoft.UI.Xaml.winmd` in the Windows App SDK 2.6.4
//! NuGet package (method order per interface, counted from 0):
//! - `ITabView` {07b509e1-1d38-551b-95f4-4732b049f6a6}: 13 put_IsAddTabButtonVisible,
//!   18/19 add/remove_TabCloseRequested, 28 get_TabItems (also bound in `bindings.rs`),
//!   36 put_CanReorderTabs, 39 get_SelectedIndex, 40 put_SelectedIndex,
//!   45/46 add/remove_SelectionChanged, 49/50 add/remove_TabDragCompleted (55 methods).
//! - `ITabViewTabCloseRequestedEventArgs` {d56ab9b2-e264-5c7e-a1cb-e41a16a6c6c6}: 1 get_Tab.
//! - `ITabViewTabDragCompletedEventArgs` {791fc623-c8f6-5102-81bd-1869cde82284}: only its IID,
//!   for the handler's signature.
//! - `SelectionChangedEventHandler` {a232390d-0e34-595e-8931-fa928a9909f4}.
//! - `IToolTipServiceStatics` {5aa38adc-9874-5e0a-8d8e-1574efc0b88f}: 8 SetToolTip.
//! - `IControl` {857d6e8a-d45a-5c69-a99c-bf6a5c54fb38}: 23 put_IsEnabled.
//! - `IUIElement`: 18 put_Visibility (matches `SetVisibility` in the generated `IUIElement_Vtbl`).
use crate::bindings::{IControl, TabView, TabViewItem, TypedEventHandler, UIElement};
use windows_core::{GUID, HRESULT, IInspectable, IUnknown, Interface, Ref, RuntimeType};

const BASE: usize = 6;
const TABVIEW_SET_IS_ADD_TAB_BUTTON_VISIBLE: usize = 13;
const TABVIEW_ADD_TAB_CLOSE_REQUESTED: usize = 18;
const TABVIEW_REMOVE_TAB_CLOSE_REQUESTED: usize = 19;
const TABVIEW_SET_CAN_REORDER_TABS: usize = 36;
const TABVIEW_GET_SELECTED_INDEX: usize = 39;
const TABVIEW_SET_SELECTED_INDEX: usize = 40;
const TABVIEW_ADD_SELECTION_CHANGED: usize = 45;
const TABVIEW_REMOVE_SELECTION_CHANGED: usize = 46;
const TABVIEW_ADD_TAB_DRAG_COMPLETED: usize = 49;
const TABVIEW_REMOVE_TAB_DRAG_COMPLETED: usize = 50;
const CLOSE_ARGS_GET_TAB: usize = 1;
const TOOLTIP_SET_TOOL_TIP: usize = 8;
const CONTROL_SET_IS_ENABLED: usize = 23;
const UIELEMENT_SET_VISIBILITY: usize = 18;

type Raw = *mut core::ffi::c_void;

/// # Safety
/// `object` must implement an interface with at least `index + 1` slots.
unsafe fn slot(object: &impl Interface, index: usize) -> usize {
    unsafe {
        let raw = object.as_raw() as *const *const usize;
        *(*raw).add(index)
    }
}

fn register<S: Interface + Clone>(
    source: &S,
    add: usize,
    remove: usize,
    handler: &impl Interface,
) -> windows_core::Result<windows_core::EventRevoker> {
    unsafe {
        let add_fn: unsafe extern "system" fn(Raw, Raw, *mut i64) -> HRESULT =
            core::mem::transmute(slot(source, BASE + add));
        let mut token = 0;
        add_fn(source.as_raw(), handler.as_raw(), &mut token).ok()?;
        let remove_fn: unsafe extern "system" fn(Raw, i64) -> HRESULT =
            core::mem::transmute(slot(source, BASE + remove));
        Ok(windows_core::EventRevoker::new(
            source.clone(),
            token,
            remove_fn,
        ))
    }
}

unsafe fn set_bool(object: &impl Interface, index: usize, value: bool) -> windows_core::Result<()> {
    unsafe {
        let set: unsafe extern "system" fn(Raw, bool) -> HRESULT =
            core::mem::transmute(slot(object, BASE + index));
        set(object.as_raw(), value).ok()
    }
}

unsafe fn set_i32(object: &impl Interface, index: usize, value: i32) -> windows_core::Result<()> {
    unsafe {
        let set: unsafe extern "system" fn(Raw, i32) -> HRESULT =
            core::mem::transmute(slot(object, BASE + index));
        set(object.as_raw(), value).ok()
    }
}

pub fn set_add_tab_button_visible(view: &TabView, value: bool) -> windows_core::Result<()> {
    unsafe { set_bool(view, TABVIEW_SET_IS_ADD_TAB_BUTTON_VISIBLE, value) }
}

pub fn set_can_reorder_tabs(view: &TabView, value: bool) -> windows_core::Result<()> {
    unsafe { set_bool(view, TABVIEW_SET_CAN_REORDER_TABS, value) }
}

pub fn selected_index(view: &TabView) -> windows_core::Result<i32> {
    unsafe {
        let get: unsafe extern "system" fn(Raw, *mut i32) -> HRESULT =
            core::mem::transmute(slot(view, BASE + TABVIEW_GET_SELECTED_INDEX));
        let mut index = -1;
        get(view.as_raw(), &mut index).ok()?;
        Ok(index)
    }
}

pub fn set_selected_index(view: &TabView, index: i32) -> windows_core::Result<()> {
    unsafe { set_i32(view, TABVIEW_SET_SELECTED_INDEX, index) }
}

/// `Control.IsEnabled`, for a disabled menu entry.
pub fn set_enabled(control: &impl Interface, value: bool) -> windows_core::Result<()> {
    let control: IControl = control.cast()?;
    unsafe { set_bool(&control, CONTROL_SET_IS_ENABLED, value) }
}

/// `UIElement.Visibility`: 0 visible, 1 collapsed.
pub fn set_visible(element: &UIElement, visible: bool) -> windows_core::Result<()> {
    unsafe { set_i32(element, UIELEMENT_SET_VISIBILITY, if visible { 0 } else { 1 }) }
}

/// A string boxed the way XAML content wants it, for a tab header or tooltip.
pub fn boxed(text: &str) -> IInspectable {
    windows_reference::IReference::<windows_core::HSTRING>::from(text).into()
}

windows_core::imp::define_interface!(
    IToolTipServiceStatics,
    IToolTipServiceStatics_Vtbl,
    0x5aa38adc_9874_5e0a_8d8e_1574efc0b88f
);
#[repr(C)]
pub struct IToolTipServiceStatics_Vtbl {
    base__: windows_core::IInspectable_Vtbl,
}
struct ToolTipService;
impl windows_core::RuntimeName for ToolTipService {
    const NAME: &'static str = "Microsoft.UI.Xaml.Controls.ToolTipService";
}

/// `ToolTipService.SetToolTip(element, text)`.
pub fn set_tooltip(element: &impl Interface, text: &str) -> windows_core::Result<()> {
    let statics = windows_core::factory::<ToolTipService, IToolTipServiceStatics>()?;
    let target: crate::bindings::DependencyObject = element.cast()?;
    let value = boxed(text);
    unsafe {
        let set: unsafe extern "system" fn(Raw, Raw, Raw) -> HRESULT =
            core::mem::transmute(slot(&statics, BASE + TOOLTIP_SET_TOOL_TIP));
        set(statics.as_raw(), target.as_raw(), value.as_raw()).ok()
    }
}

/// The same COM object, whichever interfaces the two pointers are.
pub fn same_object(a: &impl Interface, b: &impl Interface) -> bool {
    match (a.cast::<IUnknown>(), b.cast::<IUnknown>()) {
        (Ok(a), Ok(b)) => a.as_raw() == b.as_raw(),
        _ => false,
    }
}

// TabCloseRequested's args, as a class, so `TypedEventHandler<TabView, _>` gets its real IID.
windows_core::imp::define_interface!(
    ITabViewTabCloseRequestedEventArgs,
    ITabViewTabCloseRequestedEventArgs_Vtbl,
    0xd56ab9b2_e264_5c7e_a1cb_e41a16a6c6c6
);
impl RuntimeType for ITabViewTabCloseRequestedEventArgs {
    const SIGNATURE: windows_core::imp::ConstBuffer =
        windows_core::imp::ConstBuffer::for_interface::<Self>();
}
#[repr(C)]
pub struct ITabViewTabCloseRequestedEventArgs_Vtbl {
    base__: windows_core::IInspectable_Vtbl,
}
#[repr(transparent)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TabViewTabCloseRequestedEventArgs(IUnknown);
windows_core::imp::interface_hierarchy!(TabViewTabCloseRequestedEventArgs, IUnknown, IInspectable);
impl RuntimeType for TabViewTabCloseRequestedEventArgs {
    const SIGNATURE: windows_core::imp::ConstBuffer = windows_core::imp::ConstBuffer::for_class::<
        Self,
        ITabViewTabCloseRequestedEventArgs,
    >();
}
unsafe impl Interface for TabViewTabCloseRequestedEventArgs {
    type Vtable = ITabViewTabCloseRequestedEventArgs_Vtbl;
    const IID: GUID = <ITabViewTabCloseRequestedEventArgs as Interface>::IID;
}
impl windows_core::RuntimeName for TabViewTabCloseRequestedEventArgs {
    const NAME: &'static str = "Microsoft.UI.Xaml.Controls.TabViewTabCloseRequestedEventArgs";
}
impl TabViewTabCloseRequestedEventArgs {
    pub fn tab(&self) -> windows_core::Result<TabViewItem> {
        unsafe {
            let get: unsafe extern "system" fn(Raw, *mut Raw) -> HRESULT =
                core::mem::transmute(slot(self, BASE + CLOSE_ARGS_GET_TAB));
            let mut tab = core::ptr::null_mut();
            get(self.as_raw(), &mut tab).ok()?;
            if tab.is_null() {
                return Err(windows_core::Error::empty());
            }
            Ok(TabViewItem::from_raw(tab))
        }
    }
}

windows_core::imp::define_interface!(
    ITabViewTabDragCompletedEventArgs,
    ITabViewTabDragCompletedEventArgs_Vtbl,
    0x791fc623_c8f6_5102_81bd_1869cde82284
);
impl RuntimeType for ITabViewTabDragCompletedEventArgs {
    const SIGNATURE: windows_core::imp::ConstBuffer =
        windows_core::imp::ConstBuffer::for_interface::<Self>();
}
#[repr(C)]
pub struct ITabViewTabDragCompletedEventArgs_Vtbl {
    base__: windows_core::IInspectable_Vtbl,
}
#[repr(transparent)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TabViewTabDragCompletedEventArgs(IUnknown);
windows_core::imp::interface_hierarchy!(TabViewTabDragCompletedEventArgs, IUnknown, IInspectable);
impl RuntimeType for TabViewTabDragCompletedEventArgs {
    const SIGNATURE: windows_core::imp::ConstBuffer = windows_core::imp::ConstBuffer::for_class::<
        Self,
        ITabViewTabDragCompletedEventArgs,
    >();
}
unsafe impl Interface for TabViewTabDragCompletedEventArgs {
    type Vtable = ITabViewTabDragCompletedEventArgs_Vtbl;
    const IID: GUID = <ITabViewTabDragCompletedEventArgs as Interface>::IID;
}
impl windows_core::RuntimeName for TabViewTabDragCompletedEventArgs {
    const NAME: &'static str = "Microsoft.UI.Xaml.Controls.TabViewTabDragCompletedEventArgs";
}

fn typed<T, U, F>(handler: F) -> TypedEventHandler<T, U>
where
    T: RuntimeType + 'static,
    U: RuntimeType + 'static,
    F: Fn(Ref<T>, Ref<U>) + 'static,
{
    let com = windows_core::imp::DelegateBox::<TypedEventHandler<T, U>, F>::new(
        &crate::bindings::TypedEventHandlerBox::<T, U, F>::VTABLE,
        handler,
    );
    unsafe { core::mem::transmute(windows_core::imp::box_new(com)) }
}

/// `TabView.TabCloseRequested`: the close button on a tab (or the TabView's own Ctrl+F4).
pub fn tab_close_requested<F>(view: &TabView, handler: F) -> windows_core::Result<windows_core::EventRevoker>
where
    F: Fn(Ref<TabView>, Ref<TabViewTabCloseRequestedEventArgs>) + 'static,
{
    let handler = typed::<TabView, TabViewTabCloseRequestedEventArgs, F>(handler);
    register(
        view,
        TABVIEW_ADD_TAB_CLOSE_REQUESTED,
        TABVIEW_REMOVE_TAB_CLOSE_REQUESTED,
        &handler,
    )
}

/// `TabView.TabDragCompleted`: a drag (a reorder, here) ended.
pub fn tab_drag_completed<F>(view: &TabView, handler: F) -> windows_core::Result<windows_core::EventRevoker>
where
    F: Fn(Ref<TabView>, Ref<TabViewTabDragCompletedEventArgs>) + 'static,
{
    let handler = typed::<TabView, TabViewTabDragCompletedEventArgs, F>(handler);
    register(
        view,
        TABVIEW_ADD_TAB_DRAG_COMPLETED,
        TABVIEW_REMOVE_TAB_DRAG_COMPLETED,
        &handler,
    )
}

// `SelectionChangedEventHandler(object sender, SelectionChangedEventArgs e)`, a plain delegate.
windows_core::imp::define_interface!(
    SelectionChangedEventHandler,
    SelectionChangedEventHandler_Vtbl,
    0xa232390d_0e34_595e_8931_fa928a9909f4
);
impl RuntimeType for SelectionChangedEventHandler {
    const SIGNATURE: windows_core::imp::ConstBuffer =
        windows_core::imp::ConstBuffer::for_interface::<Self>();
}
#[repr(C)]
pub struct SelectionChangedEventHandler_Vtbl {
    base__: windows_core::IUnknown_Vtbl,
    invoke: unsafe extern "system" fn(Raw, Raw, Raw) -> HRESULT,
}
struct SelectionChangedBox<F: Fn() + 'static>(core::marker::PhantomData<(fn() -> F,)>);
impl<F: Fn() + 'static> SelectionChangedBox<F> {
    const VTABLE: SelectionChangedEventHandler_Vtbl = SelectionChangedEventHandler_Vtbl {
        base__: windows_core::IUnknown_Vtbl {
            QueryInterface:
                windows_core::imp::DelegateBox::<SelectionChangedEventHandler, F>::QueryInterface,
            AddRef: windows_core::imp::DelegateBox::<SelectionChangedEventHandler, F>::AddRef,
            Release: windows_core::imp::DelegateBox::<SelectionChangedEventHandler, F>::Release,
        },
        invoke: Self::invoke,
    };
    unsafe extern "system" fn invoke(this: Raw, _sender: Raw, _args: Raw) -> HRESULT {
        unsafe {
            let this = &mut *(this as *mut Raw
                as *mut windows_core::imp::DelegateBox<SelectionChangedEventHandler, F>);
            (this.invoke)();
            HRESULT(0)
        }
    }
}

/// `TabView.SelectionChanged`. The handler reads the selection itself.
pub fn selection_changed<F>(view: &TabView, handler: F) -> windows_core::Result<windows_core::EventRevoker>
where
    F: Fn() + 'static,
{
    let handler: SelectionChangedEventHandler = {
        let com = windows_core::imp::DelegateBox::<SelectionChangedEventHandler, F>::new(
            &SelectionChangedBox::<F>::VTABLE,
            handler,
        );
        unsafe { core::mem::transmute(windows_core::imp::box_new(com)) }
    };
    register(
        view,
        TABVIEW_ADD_SELECTION_CHANGED,
        TABVIEW_REMOVE_SELECTION_CHANGED,
        &handler,
    )
}
