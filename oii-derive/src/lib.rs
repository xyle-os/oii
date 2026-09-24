// derive FromNode for a struct with named fields
// reads each field with Node::field. Option fields use field_opt
use proc_macro::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Fields, LitStr, Type, parse_macro_input};

#[proc_macro_derive(FromNode, attributes(oii))]
pub fn derive_from_node(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    expand(input)
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}

fn expand(input: DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    let name = &input.ident;
    let (ig, tg, wc) = input.generics.split_for_impl();

    let data = match &input.data {
        Data::Struct(s) => s,
        _ => {
            return Err(syn::Error::new_spanned(
                &input.ident,
                "FromNode only works on structs",
            ));
        }
    };
    let named = match &data.fields {
        Fields::Named(f) => &f.named,
        _ => {
            return Err(syn::Error::new_spanned(
                &input.ident,
                "FromNode wants named fields",
            ));
        }
    };

    let mut assigns = Vec::new();
    for field in named {
        let ident = field.ident.as_ref().unwrap();
        let mut skip = false;
        let mut default = false;
        let mut rename: Option<String> = None;
        for attr in &field.attrs {
            if !attr.path().is_ident("oii") {
                continue;
            }
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("skip") {
                    skip = true;
                    Ok(())
                } else if meta.path.is_ident("default") {
                    default = true;
                    Ok(())
                } else if meta.path.is_ident("rename") {
                    let s: LitStr = meta.value()?.parse()?;
                    rename = Some(s.value());
                    Ok(())
                } else {
                    Err(meta.error("unknown oii attr. want skip default rename"))
                }
            })?;
        }

        if skip {
            assigns.push(quote! { #ident: ::core::default::Default::default() });
            continue;
        }

        let ty = &field.ty;
        let raw = ident.to_string();
        let key = rename.unwrap_or_else(|| raw.strip_prefix("r#").unwrap_or(&raw).to_string());

        let expr = if let Some(inner) = option_inner(ty) {
            quote! { n.field_opt::<#inner>(#key)? }
        } else if default {
            quote! { n.field_or(#key, ::core::default::Default::default())? }
        } else {
            quote! { n.field(#key)? }
        };
        assigns.push(quote! { #ident: #expr });
    }

    Ok(quote! {
        impl #ig ::oii::FromNode for #name #tg #wc {
            fn from_node(n: &::oii::Node) -> ::core::result::Result<Self, ::oii::DecodeError> {
                Ok(Self { #(#assigns),* })
            }
        }
    })
}

// Some(inner) for Option<inner> else None
fn option_inner(ty: &Type) -> Option<&Type> {
    let Type::Path(tp) = ty else {
        return None;
    };
    let seg = tp.path.segments.last()?;
    if seg.ident != "Option" {
        return None;
    }
    let syn::PathArguments::AngleBracketed(a) = &seg.arguments else {
        return None;
    };
    if a.args.len() != 1 {
        return None;
    }
    match a.args.first()? {
        syn::GenericArgument::Type(t) => Some(t),
        _ => None,
    }
}
