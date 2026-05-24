use proc_macro::TokenStream;
use syn::{parse_macro_input, ItemFn};

/// Marks a function as a shadow candidate for performance-gated verification.
///
/// The function is passed through unchanged at compile time.
/// When the `praetor_bench` feature is enabled, a registration module is
/// generated so that `praetor verify --shadow` can discover this function
/// and generate a benchmark harness comparing it against the original.
///
/// # Usage
///
/// ```ignore
/// #[praetor::shadow(original = "collect_facts")]
/// fn collect_facts_v2(node: Node, ctx: &mut FactContext) {
///     // refactored logic
/// }
/// ```
#[proc_macro_attribute]
pub fn shadow(attr: TokenStream, item: TokenStream) -> TokenStream {
    let input_fn = parse_macro_input!(item as ItemFn);
    let fn_name = input_fn.sig.ident.clone();

    // Parse the original function name from the attribute argument
    // Supports: original = "fn_name" or just "fn_name"
    let attr_str = attr.to_string();
    let original_name = if attr_str.contains("original =") {
        attr_str
            .split('=')
            .nth(1)
            .unwrap_or(&attr_str)
            .trim()
            .trim_matches('"')
            .trim()
            .to_string()
    } else if !attr_str.is_empty() {
        attr_str.trim_matches('"').to_string()
    } else {
        // Default: strip trailing _shadow or _v2 suffix to guess original
        let name_str = fn_name.to_string();
        if let Some(base) = name_str.strip_suffix("_shadow") {
            base.to_string()
        } else if let Some(base) = name_str.strip_suffix("_v2") {
            base.to_string()
        } else if let Some(base) = name_str.strip_suffix("_v3") {
            base.to_string()
        } else {
            name_str
        }
    };

    // Generate the benchmark registration module (only with praetor_bench feature)
    let reg_name = syn::Ident::new(
        &format!("__praetor_shadow_{}", fn_name),
        fn_name.span(),
    );
    let reg_tokens = quote::quote! {
        #[cfg(feature = "praetor_bench")]
        #[doc(hidden)]
        mod #reg_name {
            /// Returns the shadow function name and its original.
            #[no_mangle]
            pub extern "C" fn __praetor_shadow_info() -> &'static [(&'static str, &'static str)] {
                &[(stringify!(#fn_name), #original_name)]
            }
        }
    };

    let expanded = quote::quote! {
        #[allow(dead_code)]
        #input_fn
        #reg_tokens
    };

    TokenStream::from(expanded)
}