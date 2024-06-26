use proc_macro::TokenStream;
use quote::quote;

/// Derive macro for BusEvent. It implements as_any method that returns reference to dyn Any.
///
/// # Examples
/// ``` no_run
/// use event_bus::BusEvent;
/// use event_bus_macros::Event;
///
/// #[derive(Event)]
/// struct MyEvent {
///    id: u32,
/// }
/// ```
#[proc_macro_derive(Event)]
pub fn event_bus_event_derive(input: TokenStream) -> TokenStream {
    let ast: syn::DeriveInput =
        syn::parse(input).expect("failed to parse input into a DeriveInput");
    let (impl_generics, ty_generics, where_clause) = ast.generics.split_for_impl();

    let name = &ast.ident;
    let gen = quote! {
        impl #impl_generics event_bus::BusEvent for #name #ty_generics #where_clause {
            fn as_any(&self) -> &dyn core::any::Any {
                self
            }
        }
    };

    gen.into()
}
