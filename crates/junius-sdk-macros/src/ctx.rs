//! `#[derive(PluginCtx)]` (M07): wire a plugin's state struct into the per-request
//! context machinery.
//!
//! For `#[derive(PluginCtx)] struct HelloState<P = ()> { #[repo] greetings:
//! HelloRepo<P> }` it emits a `Clone` impl (free of any `P: Clone` bound) and a
//! [`BuildState`] impl that constructs each `#[repo]` field from the request
//! resources + caller. The actual `FromRequestParts` extractor for
//! `PluginContext<HelloState<P>, P>` is a single blanket impl in `junius-sdk`
//! (the orphan rule forbids implementing it here, since `PluginContext` is
//! foreign). Only `#[repo]` fields are supported in M07.

use proc_macro2::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Fields};

pub fn derive(input: TokenStream) -> TokenStream {
    let input: DeriveInput = match syn::parse2(input) {
        Ok(i) => i,
        Err(e) => return e.to_compile_error(),
    };

    let ident = &input.ident;

    // The permission-witness type param (the struct's first type param).
    let Some(param) = input
        .generics
        .type_params()
        .next()
        .map(|tp| tp.ident.clone())
    else {
        return syn::Error::new_spanned(
            &input.generics,
            "PluginCtx requires a permission-witness type parameter, e.g. `struct S<P = ()>`",
        )
        .to_compile_error();
    };

    // Collect fields; require every one to be `#[repo]` (M07 scope).
    let Data::Struct(data) = &input.data else {
        return syn::Error::new_spanned(ident, "PluginCtx can only derive on a struct")
            .to_compile_error();
    };
    let Fields::Named(named) = &data.fields else {
        return syn::Error::new_spanned(ident, "PluginCtx requires named fields")
            .to_compile_error();
    };

    let mut field_idents = Vec::new();
    let mut field_tys = Vec::new();
    for field in &named.named {
        let is_repo = field.attrs.iter().any(|a| a.path().is_ident("repo"));
        if !is_repo {
            return syn::Error::new_spanned(
                field,
                "PluginCtx supports only `#[repo]` fields in M07",
            )
            .to_compile_error();
        }
        let Some(ident) = field.ident.clone() else {
            continue; // unreachable: Fields::Named always has idents
        };
        field_idents.push(ident);
        field_tys.push(field.ty.clone());
    }

    quote! {
        impl<#param> ::core::clone::Clone for #ident<#param> {
            fn clone(&self) -> Self {
                Self {
                    #( #field_idents: ::core::clone::Clone::clone(&self.#field_idents), )*
                }
            }
        }

        impl<#param> ::junius_sdk::BuildState for #ident<#param> {
            fn build(
                resources: &::junius_sdk::PluginResources,
                user: ::core::option::Option<&::junius_sdk::User>,
            ) -> Self {
                Self {
                    #( #field_idents: <#field_tys>::new(
                        resources.db(),
                        user.cloned(),
                        ::core::clone::Clone::clone(&resources.audit),
                    ), )*
                }
            }
        }
    }
}
