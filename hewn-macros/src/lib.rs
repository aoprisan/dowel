//! Derive macro for the `hewn` dependency-wiring convention.
//!
//! See the `hewn` crate for the documented expansion. This crate is an
//! implementation detail; depend on `hewn`, not on `hewn-macros`.

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{quote, ToTokens};
use syn::{
    parse_macro_input, parse_quote, Data, DeriveInput, Error, Fields, GenericParam, Path, Type,
};

/// Derive `hewn::Wire<__Ctx>` for a struct.
///
/// Each field is wired from the context unless annotated:
/// - `#[wire(skip)]` — construct with `Default::default()`, add no bound.
/// - `#[wire(with = path)]` — construct with `path(ctx)`, add no bound.
///
/// Every plain field type `F` gets a `where F: hewn::Wire<__Ctx>` bound, so a
/// missing leaf impl is a compile error at the wiring site. See the `hewn`
/// crate docs for the full expansion.
#[proc_macro_derive(Wire, attributes(wire))]
pub fn derive_wire(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    expand(input)
        .unwrap_or_else(Error::into_compile_error)
        .into()
}

/// How a single field is constructed.
enum FieldMode {
    /// Wire the field from the context; emit a `Field: Wire<__Ctx>` bound.
    Wire,
    /// `Default::default()`, no bound.
    Skip,
    /// `path(ctx)`, no bound.
    With(Path),
}

fn expand(input: DeriveInput) -> Result<TokenStream2, Error> {
    let fields = match &input.data {
        Data::Struct(s) => &s.fields,
        Data::Enum(e) => {
            return Err(Error::new_spanned(
                e.enum_token,
                "Wire can only be derived for structs",
            ))
        }
        Data::Union(u) => {
            return Err(Error::new_spanned(
                u.union_token,
                "Wire can only be derived for structs",
            ))
        }
    };

    let name = &input.ident;

    // Build the impl generics by prepending a fresh `__Ctx` to the type's own
    // generics, preserving their bounds, lifetimes, const params and where
    // clause verbatim. Then append a `Field: Wire<__Ctx>` bound per wired field.
    let mut generics = input.generics.clone();
    let where_clause = generics.make_where_clause();

    // Per-field: figure out the construction mode and accumulate bounds.
    let mut modes = Vec::with_capacity(fields.len());
    for field in fields {
        let mode = parse_field_mode(field)?;
        if let FieldMode::Wire = mode {
            let ty: &Type = &field.ty;
            where_clause
                .predicates
                .push(parse_quote!(#ty: ::hewn::Wire<__Ctx>));
        }
        modes.push(mode);
    }

    // Snapshot the (now augmented) where clause and the original generics for
    // the `for #name #ty_generics` position before we inject `__Ctx`.
    let where_clause = generics.where_clause.clone();
    let (_, ty_generics, _) = input.generics.split_for_impl();

    // Inject `__Ctx` at the front of the impl generic parameter list.
    generics
        .params
        .insert(0, GenericParam::Type(parse_quote!(__Ctx)));
    let impl_generics = {
        let params = &generics.params;
        quote!(<#params>)
    };

    // Build the per-field initializer expressions.
    let body = build_body(fields, &modes);

    Ok(quote! {
        #[automatically_derived]
        impl #impl_generics ::hewn::Wire<__Ctx> for #name #ty_generics #where_clause {
            fn wire(__ctx: &__Ctx) -> Self {
                #body
            }
        }
    })
}

/// Construct the `Self { .. }` / `Self(..)` expression for the struct body.
fn build_body(fields: &Fields, modes: &[FieldMode]) -> TokenStream2 {
    match fields {
        Fields::Named(named) => {
            let inits = named.named.iter().zip(modes).map(|(field, mode)| {
                let ident = field.ident.as_ref().expect("named field has ident");
                let expr = init_expr(&field.ty, mode);
                quote!(#ident: #expr)
            });
            quote!(Self { #(#inits),* })
        }
        Fields::Unnamed(unnamed) => {
            let inits = unnamed
                .unnamed
                .iter()
                .zip(modes)
                .map(|(field, mode)| init_expr(&field.ty, mode));
            quote!(Self ( #(#inits),* ))
        }
        Fields::Unit => quote!(Self),
    }
}

/// The initializer expression for one field given its mode.
fn init_expr(ty: &Type, mode: &FieldMode) -> TokenStream2 {
    match mode {
        FieldMode::Wire => quote!(<#ty as ::hewn::Wire<__Ctx>>::wire(__ctx)),
        FieldMode::Skip => quote!(::core::default::Default::default()),
        FieldMode::With(path) => quote!(#path(__ctx)),
    }
}

/// Parse the `#[wire(..)]` attribute(s) on a field into a [`FieldMode`].
///
/// Accepts at most one of `skip` or `with = path`. Anything else is an error
/// with a span on the offending tokens.
fn parse_field_mode(field: &syn::Field) -> Result<FieldMode, Error> {
    let mut mode: Option<FieldMode> = None;

    for attr in &field.attrs {
        if !attr.path().is_ident("wire") {
            continue;
        }

        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("skip") {
                set_mode(&mut mode, FieldMode::Skip, &meta)?;
                Ok(())
            } else if meta.path.is_ident("with") {
                let value = meta.value()?; // parses the `=`
                let path: Path = value.parse()?;
                set_mode(&mut mode, FieldMode::With(path), &meta)?;
                Ok(())
            } else {
                Err(meta.error("unknown `wire` option; expected `skip` or `with = path`"))
            }
        })?;
    }

    Ok(mode.unwrap_or(FieldMode::Wire))
}

/// Store a mode, rejecting a second conflicting `#[wire(..)]` directive.
fn set_mode(
    slot: &mut Option<FieldMode>,
    mode: FieldMode,
    meta: &syn::meta::ParseNestedMeta,
) -> Result<(), Error> {
    if slot.is_some() {
        return Err(meta.error("conflicting `wire` options; use at most one of `skip` or `with`"));
    }
    *slot = Some(mode);
    Ok(())
}


/// Derive `hewn::Wire<Ctx>` for each named field type of a context struct.
///
/// See the `hewn` crate docs for the documented expansion. Generates, per
/// field, the leaf `impl Wire<Ctx> for FieldType` that clones the field out of
/// the context. `#[context(skip)]` omits a field; two non-skipped fields of the
/// same type are a compile error (they would produce conflicting impls).
#[proc_macro_derive(Context, attributes(context))]
pub fn derive_context(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    expand_context(input)
        .unwrap_or_else(Error::into_compile_error)
        .into()
}

fn expand_context(input: DeriveInput) -> Result<TokenStream2, Error> {
    let fields = match &input.data {
        Data::Struct(s) => match &s.fields {
            Fields::Named(named) => &named.named,
            _ => {
                return Err(Error::new_spanned(
                    &input.ident,
                    "Context can only be derived for structs with named fields",
                ))
            }
        },
        Data::Enum(e) => {
            return Err(Error::new_spanned(
                e.enum_token,
                "Context can only be derived for structs with named fields",
            ))
        }
        Data::Union(u) => {
            return Err(Error::new_spanned(
                u.union_token,
                "Context can only be derived for structs with named fields",
            ))
        }
    };

    let name = &input.ident;
    let (_, ty_generics, _) = input.generics.split_for_impl();

    // Detect duplicate leaf types: two fields of the same type would emit two
    // `impl Wire<Ctx> for T`, a coherence error. Surface a readable message at
    // the second field instead.
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut impls = Vec::new();

    for field in fields {
        if context_skip(field)? {
            continue;
        }
        let ident = field.ident.as_ref().expect("named field has ident");
        let ty: &Type = &field.ty;

        if !seen.insert(ty.to_token_stream().to_string()) {
            return Err(Error::new_spanned(
                ty,
                "duplicate leaf type in context: two non-skipped fields share this \
                 type, which would produce conflicting `Wire` impls. Annotate one \
                 field with `#[context(skip)]` and wire it by hand.",
            ));
        }

        // Forward the context's generics, adding a `FieldTy: Clone` bound so a
        // non-clonable leaf is a readable error (rule 4: leaves are clonable).
        let mut generics = input.generics.clone();
        generics
            .make_where_clause()
            .predicates
            .push(parse_quote!(#ty: ::core::clone::Clone));
        let (impl_generics, _, where_clause) = generics.split_for_impl();

        impls.push(quote! {
            #[automatically_derived]
            impl #impl_generics ::hewn::Wire<#name #ty_generics> for #ty #where_clause {
                fn wire(__ctx: &#name #ty_generics) -> Self {
                    ::core::clone::Clone::clone(&__ctx.#ident)
                }
            }
        });
    }

    Ok(quote!(#(#impls)*))
}

/// Returns whether a field is annotated `#[context(skip)]`.
fn context_skip(field: &syn::Field) -> Result<bool, Error> {
    let mut skip = false;
    for attr in &field.attrs {
        if !attr.path().is_ident("context") {
            continue;
        }
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("skip") {
                skip = true;
                Ok(())
            } else {
                Err(meta.error("unknown `context` option; expected `skip`"))
            }
        })?;
    }
    Ok(skip)
}
