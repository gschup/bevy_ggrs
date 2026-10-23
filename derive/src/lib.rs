use proc_macro::TokenStream;
use quote::quote;
use syn::{DeriveInput, meta::ParseNestedMeta, parse_macro_input};

/// Derive macro for generating [`RollbackRegistration`] implementations.
///
/// # Attributes
///
/// Use `#[rollback(...)]` to configure the rollback behavior:
///
/// ## Strategy (exactly one required)
/// - `copy` — Use [`Copy`]-based snapshots
/// - `clone` — Use [`Clone`]-based snapshots
/// - `reflect` — Use [`Reflect`]-based snapshots
///
/// ## Kind (optional, default: mutable component)
/// - `resource` — Register as a resource instead of a component
/// - `immutable` — Register as an immutable component
///
/// ## Extras (optional)
/// - `eq` — With `copy` or `clone` on a component that implements `RollbackEq`:
///   a load leaves a component already holding the snapshot's bits alone
/// - `marker` — Also register entity tracking (equivalent to `rollback_entities_with`)
/// - `checksum` — Also register hash-based checksum generation
///
/// # Examples
///
/// ```rust,ignore
/// // Mutable component with Copy strategy and entity tracking
/// #[derive(Component, Rollback)]
/// #[rollback(copy, marker)]
/// struct Player;
///
/// // Resource with Clone strategy
/// #[derive(Resource, Rollback)]
/// #[rollback(resource, clone)]
/// struct GameState { ... }
///
/// // Immutable component with Copy strategy
/// #[derive(Component, Rollback)]
/// #[component(immutable)]
/// #[rollback(immutable, copy)]
/// struct Tag;
/// ```
#[proc_macro_derive(Rollback, attributes(rollback))]
pub fn derive_rollback(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    match derive_rollback_impl(input) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

#[derive(Default)]
struct RollbackAttrs {
    strategy: Option<Strategy>,
    resource: bool,
    immutable: bool,
    marker: bool,
    checksum: bool,
    eq: bool,
}

enum Strategy {
    Copy,
    Clone,
    Reflect,
}

impl RollbackAttrs {
    fn parse(&mut self, meta: ParseNestedMeta<'_>) -> syn::Result<()> {
        if meta.path.is_ident("copy") {
            self.strategy = Some(Strategy::Copy);
        } else if meta.path.is_ident("clone") {
            self.strategy = Some(Strategy::Clone);
        } else if meta.path.is_ident("reflect") {
            self.strategy = Some(Strategy::Reflect);
        } else if meta.path.is_ident("resource") {
            self.resource = true;
        } else if meta.path.is_ident("immutable") {
            self.immutable = true;
        } else if meta.path.is_ident("marker") {
            self.marker = true;
        } else if meta.path.is_ident("checksum") {
            self.checksum = true;
        } else if meta.path.is_ident("eq") {
            self.eq = true;
        } else {
            return Err(meta.error("unknown rollback attribute"));
        }
        Ok(())
    }
}

fn derive_rollback_impl(input: DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    let name = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let mut attrs = RollbackAttrs::default();

    for attr in &input.attrs {
        if attr.path().is_ident("rollback") {
            attr.parse_nested_meta(|meta| attrs.parse(meta))?;
        }
    }

    let strategy = attrs.strategy.as_ref().ok_or_else(|| {
        syn::Error::new_spanned(
            &input.ident,
            "#[rollback(...)] requires a strategy: copy, clone, or reflect",
        )
    })?;

    if attrs.resource && attrs.immutable {
        return Err(syn::Error::new_spanned(
            &input.ident,
            "#[rollback(...)] cannot combine `resource` and `immutable`",
        ));
    }

    if attrs.resource && attrs.marker {
        return Err(syn::Error::new_spanned(
            &input.ident,
            "#[rollback(...)] cannot combine `resource` and `marker` (entity tracking is for components only)",
        ));
    }

    if attrs.eq && (attrs.resource || matches!(strategy, Strategy::Reflect)) {
        return Err(syn::Error::new_spanned(
            &input.ident,
            "#[rollback(eq)] is for components with `copy` or `clone`",
        ));
    }

    let snapshot_call = match (&strategy, attrs.resource, attrs.immutable, attrs.eq) {
        (Strategy::Copy, false, false, true) => {
            quote! { app.rollback_component_with_copy_eq::<Self>(); }
        }
        (Strategy::Clone, false, false, true) => {
            quote! { app.rollback_component_with_clone_eq::<Self>(); }
        }
        (Strategy::Copy, false, true, true) => {
            quote! { app.rollback_immutable_component_with_copy_eq::<Self>(); }
        }
        (Strategy::Clone, false, true, true) => {
            quote! { app.rollback_immutable_component_with_clone_eq::<Self>(); }
        }
        (strategy, resource, immutable, _) => match (strategy, resource, immutable) {
            (Strategy::Copy, true, _) => quote! { app.rollback_resource_with_copy::<Self>(); },
            (Strategy::Clone, true, _) => quote! { app.rollback_resource_with_clone::<Self>(); },
            (Strategy::Reflect, true, _) => {
                quote! { app.rollback_resource_with_reflect::<Self>(); }
            }
            (Strategy::Copy, false, true) => {
                quote! { app.rollback_immutable_component_with_copy::<Self>(); }
            }
            (Strategy::Clone, false, true) => {
                quote! { app.rollback_immutable_component_with_clone::<Self>(); }
            }
            (Strategy::Reflect, false, true) => {
                quote! { app.rollback_immutable_component_with_reflect::<Self>(); }
            }
            (Strategy::Copy, false, false) => {
                quote! { app.rollback_component_with_copy::<Self>(); }
            }
            (Strategy::Clone, false, false) => {
                quote! { app.rollback_component_with_clone::<Self>(); }
            }
            (Strategy::Reflect, false, false) => {
                quote! { app.rollback_component_with_reflect::<Self>(); }
            }
        },
    };

    let marker_call = if attrs.marker {
        quote! { app.rollback_entities_with::<Self>(); }
    } else {
        quote! {}
    };

    let checksum_call = if attrs.checksum && attrs.resource {
        quote! { app.checksum_resource_with_hash::<Self>(); }
    } else if attrs.checksum {
        quote! { app.checksum_component_with_hash::<Self>(); }
    } else {
        quote! {}
    };

    Ok(quote! {
        impl #impl_generics bevy_ggrs::RollbackRegistration for #name #ty_generics #where_clause {
            fn register_rollback(app: &mut bevy::prelude::App) {
                use bevy_ggrs::RollbackApp as _;
                #marker_call
                #snapshot_call
                #checksum_call
            }
        }
    })
}
