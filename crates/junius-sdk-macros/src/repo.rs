//! `#[repository]` and `#[impl_repository(...)]` attribute macros (M07).
//!
//! `#[repository]` rewrites a marker struct (`pub struct HelloRepo<P = ()>;`)
//! into a real repository: it injects the private `db`/`user`/`audit`/phantom
//! fields and generates `new`, the sealed `pool()` accessor, and a `Clone` impl
//! that doesn't require `P: Clone`. (A `derive` can't add fields, so this is an
//! attribute macro.)
//!
//! `#[impl_repository(Repo)]` marks a repository's method impls and injects a
//! fresh index type-param for every `Has<Perm>` bound, so authors write the clean
//! `impl<P: Has<HelloRead>> …` while the underlying `Has<X, Idx>` trait stays
//! coherent.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::punctuated::Punctuated;
use syn::{GenericArgument, GenericParam, ItemImpl, ItemStruct, PathArguments, TypeParamBound};

/// Expand `#[repository]` on a marker struct into the full repository type.
pub fn repository(item: TokenStream) -> TokenStream {
    let input: ItemStruct = match syn::parse2(item) {
        Ok(s) => s,
        Err(e) => return e.to_compile_error(),
    };

    let vis = &input.vis;
    let ident = &input.ident;
    let generics = &input.generics;
    let (impl_g, ty_g, where_c) = generics.split_for_impl();
    let phantom_tys = generics.type_params().map(|tp| &tp.ident);
    let phantom = quote! { ::core::marker::PhantomData<( #(#phantom_tys,)* )> };

    quote! {
        #vis struct #ident #generics {
            db: ::junius_sdk::ScopedDb,
            user: ::core::option::Option<::junius_sdk::User>,
            audit: ::junius_sdk::AuditEmitter,
            _marker: #phantom,
        }

        impl #impl_g #ident #ty_g #where_c {
            /// Build the repository for a request. Called by the
            /// `#[derive(PluginCtx)]`-generated extractor, not by hand.
            #[allow(dead_code)]
            pub fn new(
                db: &::junius_sdk::PluginDb,
                user: ::core::option::Option<::junius_sdk::User>,
                audit: ::junius_sdk::AuditEmitter,
            ) -> Self {
                Self {
                    db: ::junius_sdk::ScopedDb::__from_plugin_db(db),
                    user,
                    audit,
                    _marker: ::core::marker::PhantomData,
                }
            }

            /// The SQL executor — the only path to a `&PgPool`, usable inside
            /// `#[impl_repository]` method bodies.
            #[allow(dead_code)]
            fn pool(&self) -> &::sqlx::PgPool {
                <::junius_sdk::ScopedDb as ::junius_sdk::RepoPool>::pool(&self.db)
            }

            /// The current caller, if any (for audit + ownership).
            #[allow(dead_code)]
            fn user(&self) -> ::core::option::Option<&::junius_sdk::User> {
                self.user.as_ref()
            }

            /// The audit-log writer (writes to `platform.audit_event`).
            #[allow(dead_code)]
            fn audit(&self) -> &::junius_sdk::AuditEmitter {
                &self.audit
            }
        }

        impl #impl_g ::core::clone::Clone for #ident #ty_g #where_c {
            fn clone(&self) -> Self {
                Self {
                    db: ::core::clone::Clone::clone(&self.db),
                    user: ::core::clone::Clone::clone(&self.user),
                    audit: ::core::clone::Clone::clone(&self.audit),
                    _marker: ::core::marker::PhantomData,
                }
            }
        }
    }
}

/// Expand `#[impl_repository(Repo)]`: move each single-argument `Has<Perm>` bound
/// off the impl and onto every method as `Has<Perm, __HasIdxN>` with a fresh
/// per-method index type-param. (Inherent-impl params must appear in the self
/// type, so the index can't live on the impl — but a method/function param is
/// free to be inferred. This keeps author-written impls as `impl<P: Has<X>>`.)
pub fn impl_repository(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let mut input: ItemImpl = match syn::parse2(item) {
        Ok(i) => i,
        Err(e) => return e.to_compile_error(),
    };

    // Pull the `Has<…>` bounds off the impl generics (and where-clause), keeping
    // any other bounds in place.
    let has_bounds = extract_has_bounds(&mut input.generics);

    // Re-attach them, indexed, to each method.
    for item in &mut input.items {
        if let syn::ImplItem::Fn(method) = item {
            attach_has_bounds(&mut method.sig.generics, &has_bounds);
        }
    }

    quote! { #input }
}

/// A `Has<X>` bound lifted off the impl: the bounded type (`P`) and the trait
/// bound itself (so its written path is preserved when re-attached).
struct HasBound {
    bounded: syn::Type,
    bound: syn::TraitBound,
}

fn is_single_arg_has(tb: &syn::TraitBound) -> bool {
    let Some(seg) = tb.path.segments.last() else {
        return false;
    };
    if seg.ident != "Has" {
        return false;
    }
    let PathArguments::AngleBracketed(args) = &seg.arguments else {
        return false;
    };
    args.args
        .iter()
        .filter(|a| matches!(a, GenericArgument::Type(_)))
        .count()
        == 1
}

/// Remove `Has<X>` bounds from `generics` (param-position and where-clause),
/// returning them paired with the type they bound.
fn extract_has_bounds(generics: &mut syn::Generics) -> Vec<HasBound> {
    let mut collected = Vec::new();

    for param in &mut generics.params {
        if let GenericParam::Type(tp) = param {
            let bounded: syn::Type = {
                let ident = &tp.ident;
                syn::parse_quote!(#ident)
            };
            partition_bounds(&mut tp.bounds, &bounded, &mut collected);
        }
    }

    if let Some(wc) = &mut generics.where_clause {
        let mut kept: Punctuated<syn::WherePredicate, syn::Token![,]> = Punctuated::new();
        for pred in std::mem::take(&mut wc.predicates) {
            if let syn::WherePredicate::Type(mut pt) = pred {
                partition_bounds(&mut pt.bounds, &pt.bounded_ty.clone(), &mut collected);
                if !pt.bounds.is_empty() {
                    kept.push(syn::WherePredicate::Type(pt));
                }
            } else {
                kept.push(pred);
            }
        }
        wc.predicates = kept;
    }

    collected
}

/// Split `bounds` into kept (non-`Has`) bounds, recording each `Has<X>` bound
/// against `bounded`.
fn partition_bounds(
    bounds: &mut Punctuated<TypeParamBound, syn::Token![+]>,
    bounded: &syn::Type,
    collected: &mut Vec<HasBound>,
) {
    let mut kept: Punctuated<TypeParamBound, syn::Token![+]> = Punctuated::new();
    for bound in std::mem::take(bounds) {
        match bound {
            TypeParamBound::Trait(tb) if is_single_arg_has(&tb) => {
                collected.push(HasBound {
                    bounded: bounded.clone(),
                    bound: tb,
                });
            }
            other => kept.push(other),
        }
    }
    *bounds = kept;
}

/// Add `bounded: Has<X, __HasIdxN>` to a method's generics, one fresh index
/// param per lifted bound.
fn attach_has_bounds(generics: &mut syn::Generics, has_bounds: &[HasBound]) {
    for (n, hb) in has_bounds.iter().enumerate() {
        let idx = format_ident!("__HasIdx{n}");
        let mut bound = hb.bound.clone();
        if let Some(seg) = bound.path.segments.last_mut() {
            if let PathArguments::AngleBracketed(args) = &mut seg.arguments {
                args.args
                    .push(GenericArgument::Type(syn::parse_quote!(#idx)));
            }
        }
        generics.params.push(syn::parse_quote!(#idx));
        let bounded = &hb.bounded;
        generics
            .make_where_clause()
            .predicates
            .push(syn::parse_quote!(#bounded: #bound));
    }
}
