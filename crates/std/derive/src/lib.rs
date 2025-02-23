use {
    proc_macro::TokenStream,
    quote::quote,
    std::str::FromStr,
    syn::{parse_macro_input, Data, DeriveInput, ItemFn},
};

#[proc_macro_attribute]
pub fn cw_serde(_attr: TokenStream, input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    match input.data {
        Data::Struct(_) | Data::Enum(_) => quote! {
            #[::cw_std::__private::serde_with::skip_serializing_none]
            #[derive(
                ::cw_std::__private::serde::Serialize,
                ::cw_std::__private::serde::Deserialize,
                ::std::clone::Clone,
                ::std::fmt::Debug,
                ::std::cmp::PartialEq,
                ::std::cmp::Eq,
            )]
            #[serde(deny_unknown_fields, rename_all = "snake_case", crate = "::cw_std::__private::serde")]
            #input
        },
        Data::Union(_) => panic!("Union is not supported"),
    }
    .into()
}

#[proc_macro_attribute]
pub fn entry_point(_attr: TokenStream, mut item: TokenStream) -> TokenStream {
    let cloned = item.clone();
    let function = parse_macro_input!(cloned as ItemFn);
    let name = function.sig.ident.to_string();
    let args = function.sig.inputs.len();

    // e.g. "ptr0: usize, ptr1: usize, ptr2: usize, "
    let typed_ptrs = (0..args).fold(String::new(), |acc, i| format!("{acc}ptr{i}: usize, "));
    // e.g. "ptr0, ptr1, ptr2, "
    let ptrs = (0..args).fold(String::new(), |acc, i| format!("{acc}ptr{i}, "));

    // new module to avoid conflict of function names
    let new_code = format!(r##"
        #[cfg(target_arch = "wasm32")]
        mod __wasm_export_{name} {{
            #[no_mangle]
            extern "C" fn {name}({typed_ptrs}) -> usize {{
                cw_std::do_{name}(&super::{name}, {ptrs})
            }}
        }}
    "##);

    let entry = TokenStream::from_str(&new_code).unwrap();
    item.extend(entry);
    item
}
