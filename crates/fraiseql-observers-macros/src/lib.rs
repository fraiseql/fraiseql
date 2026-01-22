//! Procedural macros for fraiseql-observers automatic instrumentation
//!
//! This crate provides macros for automatic span creation and structured logging.
//!
//! # Macros
//!
//! - `#[traced]` - Creates a span for the function and records execution
//! - `#[instrument]` - Adds structured logging with function arguments

extern crate proc_macro;

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, FnArg, ItemFn, Meta};

/// Creates an automatic span for function execution
///
/// # Example
///
/// ```ignore
/// #[traced(name = "process_event")]
/// async fn process_event(event: &Event) -> Result<()> {
///     // Automatic span creation and error tracking
///     Ok(())
/// }
/// ```
#[proc_macro_attribute]
pub fn traced(args: TokenStream, input: TokenStream) -> TokenStream {
    let input_fn = parse_macro_input!(input as ItemFn);
    let args = parse_macro_input!(args as Meta);

    // Extract function name from attribute or use function name
    let span_name = match args {
        Meta::NameValue(nv) if nv.path.is_ident("name") => {
            if let syn::Expr::Lit(expr_lit) = &nv.value {
                if let syn::Lit::Str(lit_str) = &expr_lit.lit {
                    lit_str.value()
                } else {
                    input_fn.sig.ident.to_string()
                }
            } else {
                input_fn.sig.ident.to_string()
            }
        }
        _ => input_fn.sig.ident.to_string(),
    };

    let fn_body = &input_fn.block;
    let fn_visibility = &input_fn.vis;
    let fn_sig = &input_fn.sig;

    // Check if function is async
    let is_async = input_fn.sig.asyncness.is_some();

    let expanded = if is_async {
        quote! {
            #fn_visibility #fn_sig {
                let span = tracing::debug_span!(#span_name);
                let _guard = span.enter();
                let start = std::time::Instant::now();

                let result = async {
                    #fn_body
                }.await;

                let duration_ms = start.elapsed().as_millis();
                match &result {
                    Ok(_) => {
                        tracing::debug!(duration_ms = duration_ms, "span completed successfully");
                    }
                    Err(e) => {
                        tracing::warn!(duration_ms = duration_ms, error = %e, "span failed");
                    }
                }

                result
            }
        }
    } else {
        quote! {
            #fn_visibility #fn_sig {
                let span = tracing::debug_span!(#span_name);
                let _guard = span.enter();
                let start = std::time::Instant::now();

                let result = {
                    #fn_body
                };

                let duration_ms = start.elapsed().as_millis();
                match &result {
                    Ok(_) => {
                        tracing::debug!(duration_ms = duration_ms, "span completed successfully");
                    }
                    Err(e) => {
                        tracing::warn!(duration_ms = duration_ms, error = %e, "span failed");
                    }
                }

                result
            }
        }
    };

    TokenStream::from(expanded)
}

/// Adds structured logging with function arguments
///
/// # Example
///
/// ```ignore
/// #[instrument]
/// fn process_data(id: u32, name: String) -> Result<()> {
///     // Automatically logs id and name
///     Ok(())
/// }
/// ```
#[proc_macro_attribute]
pub fn instrument(_args: TokenStream, input: TokenStream) -> TokenStream {
    let input_fn = parse_macro_input!(input as ItemFn);

    let fn_name = &input_fn.sig.ident;
    let fn_body = &input_fn.block;
    let fn_visibility = &input_fn.vis;
    let fn_sig = &input_fn.sig;

    // Extract parameter names and types
    let mut field_captures = Vec::new();
    for input in &fn_sig.inputs {
        if let FnArg::Typed(pat_type) = input {
            if let syn::Pat::Ident(pat_ident) = &*pat_type.pat {
                let ident = &pat_ident.ident;
                field_captures.push(quote! {
                    #ident = ?#ident
                });
            }
        }
    }

    let is_async = fn_sig.asyncness.is_some();

    let expanded = if is_async {
        if field_captures.is_empty() {
            quote! {
                #fn_visibility #fn_sig {
                    tracing::debug!(target: stringify!(#fn_name), "function entered");
                    #fn_body
                }
            }
        } else {
            quote! {
                #fn_visibility #fn_sig {
                    tracing::debug!(target: stringify!(#fn_name), #(#field_captures),*, "function entered");
                    #fn_body
                }
            }
        }
    } else if field_captures.is_empty() {
        quote! {
            #fn_visibility #fn_sig {
                tracing::debug!(target: stringify!(#fn_name), "function entered");
                #fn_body
            }
        }
    } else {
        quote! {
            #fn_visibility #fn_sig {
                tracing::debug!(target: stringify!(#fn_name), #(#field_captures),*, "function entered");
                #fn_body
            }
        }
    };

    TokenStream::from(expanded)
}

#[cfg(test)]
mod tests {
    // Macro tests are typically integration tests
    // See crate root for test examples
}
