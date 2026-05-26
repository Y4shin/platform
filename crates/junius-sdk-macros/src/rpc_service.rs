//! `#[rpc_service(<ServiceTrait>)]` — attribute macro that turns an inherent
//! `impl <RpcStruct> { … }` into the `impl <ServiceTrait> for <RpcStruct>` block
//! that `connectrpc`'s generated service trait expects, hiding the boilerplate
//! `RequestContext` parameter + `from_rpc` call (M15 Stage 3).
//!
//! "Plumbing-only": the macro reads the ctx parameter's *written* type verbatim
//! and never invents a witness — authors write the alias path themselves, so
//! the proto-derived requirement is visible at the call site (decision #1 in
//! `docs/impl/17-M15-rpc-service-macro.md`).

use proc_macro2::TokenStream;
use quote::{ToTokens, quote, quote_spanned};
use syn::spanned::Spanned as _;
use syn::{FnArg, ImplItem, ImplItemFn, ItemImpl, Pat, Path, parse_quote, parse2};

/// Expand `#[rpc_service(ServiceName)] impl RpcStruct { … }` into the
/// `connectrpc`-shaped `impl ServiceName for RpcStruct { … }`. See module docs.
pub fn rpc_service(attr: TokenStream, item: TokenStream) -> TokenStream {
    // The macro arg is the path to the proto-generated service trait — usually
    // a bare `EventService` (re-exported as `proto::events::v1::EventService`
    // in the caller).
    let service_path: Path = match parse2(attr) {
        Ok(p) => p,
        Err(e) => return e.to_compile_error(),
    };

    let mut item_impl: ItemImpl = match parse2(item) {
        Ok(i) => i,
        Err(e) => return e.to_compile_error(),
    };

    // Must be an inherent impl; the macro adds the trait for you.
    if let Some((bang, trait_path, _for_kw)) = &item_impl.trait_ {
        let written = quote!(#bang #trait_path);
        return syn::Error::new_spanned(
            trait_path,
            format!(
                "#[rpc_service] expects an inherent `impl <RpcStruct> {{ … }}` — got `impl {written} for …`. \
                 Drop the trait; the macro emits it for you.",
            ),
        )
        .to_compile_error();
    }

    // Rewrite each fn item; non-fn items (consts, types) pass through.
    let mut errors = TokenStream::new();
    for item in &mut item_impl.items {
        if let ImplItem::Fn(method) = item {
            if let Err(e) = rewrite_method(method) {
                errors.extend(e.to_compile_error());
            }
        }
    }
    if !errors.is_empty() {
        return errors;
    }

    let attrs = &item_impl.attrs;
    let generics = &item_impl.generics;
    let where_clause = &item_impl.generics.where_clause;
    let self_ty = &item_impl.self_ty;
    let items = &item_impl.items;
    quote! {
        #(#attrs)*
        impl #generics #service_path for #self_ty #where_clause {
            #(#items)*
        }
    }
}

/// Rewrite a single handler method in-place:
/// - replace the ctx parameter with `__ctx: ::connectrpc::RequestContext`,
/// - prepend `let <ctx_ident> = <written_ty>::from_rpc(&__ctx)?;` to the body,
///   so the author's source still references their named ctx.
///
/// Spans on the prepended statement point at the author's ctx parameter — so a
/// witness-resolution failure ("`Has<EventsWrite>` not satisfied") highlights
/// the *parameter type*, where the alias path is named, not somewhere deep in
/// the macro output.
fn rewrite_method(method: &mut ImplItemFn) -> syn::Result<()> {
    let sig_span = method.sig.span();
    let inputs = &mut method.sig.inputs;

    // Must take `&self` (handler methods on a trait do).
    let has_receiver = matches!(inputs.first(), Some(FnArg::Receiver(_)));
    if !has_receiver {
        return Err(syn::Error::new(
            sig_span,
            "#[rpc_service] handler methods must take `&self`",
        ));
    }

    // The ctx parameter is positional: first non-`&self`.
    let Some(ctx_arg) = inputs.iter().nth(1).cloned() else {
        return Err(syn::Error::new(
            sig_span,
            "#[rpc_service] handler is missing its ctx parameter \
             (expected `<ctx_name>: <PluginCtx>::<crate::__rpc_requires::...>`)",
        ));
    };

    let FnArg::Typed(ctx_pat_type) = &ctx_arg else {
        return Err(syn::Error::new(
            ctx_arg.span(),
            "#[rpc_service] ctx parameter must be a typed `name: Type` binding",
        ));
    };
    let Pat::Ident(ctx_pat_ident) = ctx_pat_type.pat.as_ref() else {
        return Err(syn::Error::new(
            ctx_pat_type.pat.span(),
            "#[rpc_service] ctx parameter must be a plain identifier (e.g. `ctx`, `ectx`)",
        ));
    };

    let ctx_ident = ctx_pat_ident.ident.clone();
    let written_ty = ctx_pat_type.ty.clone();
    let ctx_span = ctx_pat_type.span();

    // Coarse forward-compat guard: only unary RPCs (return `ServiceResult<…>`).
    // Streaming is a separate macro shape (M15 follow-up).
    let return_tokens = method.sig.output.to_token_stream().to_string();
    if !return_tokens.contains("ServiceResult") {
        return Err(syn::Error::new(
            method.sig.output.span(),
            "#[rpc_service] only supports unary methods returning `ServiceResult<…>` (streaming is a follow-up). \
             If the return type is `ServiceResult` re-exported under a different name, alias it directly.",
        ));
    }

    // Build the new ctx parameter with a fixed internal name; we keep the
    // author's ident as the binding in the prepended `let`.
    let new_ctx: FnArg = parse_quote! {
        __ctx: ::connectrpc::RequestContext
    };
    if let Some(slot) = inputs.iter_mut().nth(1) {
        *slot = new_ctx;
    }

    // Prepend the resolution. Span-tied to the ctx parameter so witness-trait
    // errors land on the type the author wrote.
    let stmt: syn::Stmt = parse_quote! {
        let #ctx_ident = <#written_ty>::from_rpc(&__ctx)?;
    };
    // Re-quote the stmt at the ctx span so errors highlight the user's code.
    let stmt_tokens = quote_spanned! {ctx_span=> #stmt };
    let stmt_at_span: syn::Stmt = parse2(stmt_tokens).unwrap_or(stmt);
    method.block.stmts.insert(0, stmt_at_span);

    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    fn expand(attr: TokenStream, item: TokenStream) -> String {
        rpc_service(attr, item).to_string()
    }

    #[test]
    fn rewrites_ctx_param_and_prepends_from_rpc() {
        let out = expand(
            quote! { EventService },
            quote! {
                impl EventRpc {
                    async fn list_events(
                        &self,
                        ectx: EventCtx<crate::__rpc_requires::event_service::ListEvents>,
                        _request: OwnedListEventsRequestView,
                    ) -> ServiceResult<impl Encodable<pb::ListEventsResponse>> {
                        let events = ectx.state.events.list().await?;
                        Ok(Response::new(pb::ListEventsResponse::default()))
                    }
                }
            },
        );

        // Emitted shape: trait impl for the same self type.
        assert!(out.contains("impl EventService for EventRpc"), "got: {out}");
        // ctx param renamed.
        assert!(
            out.contains(":: connectrpc :: RequestContext"),
            "ctx not rewritten — got: {out}"
        );
        // Prepended `let ectx = <…>::from_rpc(& __ctx) ?;`.
        assert!(
            out.contains("let ectx = <") && out.contains(":: from_rpc (& __ctx)"),
            "from_rpc prepend missing — got: {out}"
        );
        // Original body preserved (the repo call survives).
        assert!(
            out.contains("ectx . state . events . list"),
            "body lost — got: {out}"
        );
    }

    #[test]
    fn rejects_trait_impl_block() {
        let out = expand(
            quote! { EventService },
            quote! {
                impl EventService for EventRpc {
                    async fn list_events(
                        &self,
                        ctx: RequestContext,
                        _request: OwnedListEventsRequestView,
                    ) -> ServiceResult<impl Encodable<pb::ListEventsResponse>> {
                        unimplemented!()
                    }
                }
            },
        );
        assert!(
            out.contains("compile_error"),
            "expected compile_error for trait impl input — got: {out}"
        );
        assert!(
            out.contains("inherent"),
            "expected `inherent` in the diagnostic — got: {out}"
        );
    }

    #[test]
    fn rejects_method_missing_ctx_parameter() {
        let out = expand(
            quote! { EventService },
            quote! {
                impl EventRpc {
                    async fn list_events(&self)
                        -> ServiceResult<impl Encodable<pb::ListEventsResponse>>
                    {
                        unimplemented!()
                    }
                }
            },
        );
        assert!(
            out.contains("compile_error") && out.contains("ctx parameter"),
            "expected missing-ctx compile_error — got: {out}"
        );
    }

    #[test]
    fn rejects_non_unary_return_shape() {
        let out = expand(
            quote! { EventService },
            quote! {
                impl EventRpc {
                    async fn stream_events(
                        &self,
                        ectx: EventCtx<()>,
                        _request: OwnedStreamEventsRequestView,
                    ) -> impl Stream<Item = Result<EventChunk, ConnectError>> {
                        unimplemented!()
                    }
                }
            },
        );
        assert!(
            out.contains("compile_error") && out.contains("unary"),
            "expected streaming-rejection compile_error — got: {out}"
        );
    }

    #[test]
    fn passes_through_non_fn_items() {
        let out = expand(
            quote! { EventService },
            quote! {
                impl EventRpc {
                    const TAG: &'static str = "event";
                    async fn list_events(
                        &self,
                        ectx: EventCtx<()>,
                        _request: OwnedListEventsRequestView,
                    ) -> ServiceResult<impl Encodable<pb::ListEventsResponse>> {
                        let _ = ectx;
                        unimplemented!()
                    }
                }
            },
        );
        assert!(
            out.contains("const TAG :"),
            "expected const item preserved — got: {out}"
        );
    }
}
