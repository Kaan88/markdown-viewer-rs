//! `TreeView.ItemInvoked`, which the trimmed `bindings.rs` leaves as an unbound slot. Called by vtable
//! slot index like `tabs.rs`; the indices and the IID come from `ildasm`-style reading of
//! `Microsoft.UI.Xaml.winmd` in Windows App SDK 2.6.4:
//! - `ITreeView` {1bef9af4-712c-50ef-9bb4-881b975232ab}: 7/8 add/remove_ItemInvoked (the same
//!   interface the generated `Expanding` binding uses at 9/10).
//! - `ITreeViewItemInvokedEventArgs` {1a05853c-b101-542c-9a1e-775a044c4652}: 0 get_InvokedItem.
use crate::bindings::{TreeView, TypedEventHandler};
use crate::tabs::{BASE, Raw, register, slot, typed};
use windows_core::{GUID, HRESULT, IInspectable, IUnknown, Interface, Ref, RuntimeType};

const TREEVIEW_ADD_ITEM_INVOKED: usize = 7;
const TREEVIEW_REMOVE_ITEM_INVOKED: usize = 8;
const INVOKED_ARGS_GET_INVOKED_ITEM: usize = 0;

windows_core::imp::define_interface!(
    ITreeViewItemInvokedEventArgs,
    ITreeViewItemInvokedEventArgs_Vtbl,
    0x1a05853c_b101_542c_9a1e_775a044c4652
);
impl RuntimeType for ITreeViewItemInvokedEventArgs {
    const SIGNATURE: windows_core::imp::ConstBuffer =
        windows_core::imp::ConstBuffer::for_interface::<Self>();
}
#[repr(C)]
pub struct ITreeViewItemInvokedEventArgs_Vtbl {
    base__: windows_core::IInspectable_Vtbl,
}
#[repr(transparent)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TreeViewItemInvokedEventArgs(IUnknown);
windows_core::imp::interface_hierarchy!(TreeViewItemInvokedEventArgs, IUnknown, IInspectable);
impl RuntimeType for TreeViewItemInvokedEventArgs {
    const SIGNATURE: windows_core::imp::ConstBuffer = windows_core::imp::ConstBuffer::for_class::<
        Self,
        ITreeViewItemInvokedEventArgs,
    >();
}
unsafe impl Interface for TreeViewItemInvokedEventArgs {
    type Vtable = ITreeViewItemInvokedEventArgs_Vtbl;
    const IID: GUID = <ITreeViewItemInvokedEventArgs as Interface>::IID;
}
impl windows_core::RuntimeName for TreeViewItemInvokedEventArgs {
    const NAME: &'static str = "Microsoft.UI.Xaml.Controls.TreeViewItemInvokedEventArgs";
}
impl TreeViewItemInvokedEventArgs {
    /// The tree node that was clicked, as the object XAML hands over.
    pub fn invoked_item(&self) -> windows_core::Result<IInspectable> {
        unsafe {
            let get: unsafe extern "system" fn(Raw, *mut Raw) -> HRESULT =
                core::mem::transmute(slot(self, BASE + INVOKED_ARGS_GET_INVOKED_ITEM));
            let mut item = core::ptr::null_mut();
            get(self.as_raw(), &mut item).ok()?;
            if item.is_null() {
                return Err(windows_core::Error::empty());
            }
            Ok(IInspectable::from_raw(item))
        }
    }
}

/// `TreeView.ItemInvoked`: a row was clicked or activated with the keyboard.
pub fn item_invoked<F>(view: &TreeView, handler: F) -> windows_core::Result<windows_core::EventRevoker>
where
    F: Fn(Ref<TreeView>, Ref<TreeViewItemInvokedEventArgs>) + 'static,
{
    let handler: TypedEventHandler<TreeView, TreeViewItemInvokedEventArgs> =
        typed::<TreeView, TreeViewItemInvokedEventArgs, F>(handler);
    register(
        view,
        TREEVIEW_ADD_ITEM_INVOKED,
        TREEVIEW_REMOVE_ITEM_INVOKED,
        &handler,
    )
}
