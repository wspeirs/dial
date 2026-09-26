use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput};

#[proc_macro_derive(Parser, attributes(grammar, grammar_inline))]
pub fn derive_parser(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as DeriveInput);
    let name = &ast.ident;

    let mut grammar_paths = Vec::new();
    let mut grammar_sources = Vec::new();

    for attr in &ast.attrs {
        if attr.path().is_ident("grammar") {
            let lit_str: syn::LitStr = match &attr.meta {
                syn::Meta::NameValue(nv) => {
                    if let syn::Expr::Lit(syn::ExprLit {
                        lit: syn::Lit::Str(s),
                        ..
                    }) = &nv.value
                    {
                        s.clone()
                    } else {
                        return syn::Error::new_spanned(
                            &nv.value,
                            "expected string literal for #[grammar = \"...\"]",
                        )
                        .to_compile_error()
                        .into();
                    }
                }
                syn::Meta::List(list) => match list.parse_args::<syn::LitStr>() {
                    Ok(s) => s,
                    Err(e) => return e.to_compile_error().into(),
                },
                _ => {
                    return syn::Error::new_spanned(
                        attr,
                        "expected #[grammar = \"...\"] or #[grammar(\"...\")]",
                    )
                    .to_compile_error()
                    .into();
                }
            };

            let rel_path = lit_str.value();
            let manifest_dir =
                std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string());
            let root = std::path::Path::new(&manifest_dir);
            let path1 = root.join(&rel_path);
            let path2 = root.join("src").join(&rel_path);
            let resolved = if path1.exists() {
                path1
            } else if path2.exists() {
                path2
            } else {
                return syn::Error::new_spanned(
                    &lit_str,
                    format!(
                        "grammar file '{}' not found in '{}' or '{}/src'",
                        rel_path, manifest_dir, manifest_dir
                    ),
                )
                .to_compile_error()
                .into();
            };

            let abs_path_str = match resolved.canonicalize() {
                Ok(p) => p.display().to_string(),
                Err(e) => {
                    return syn::Error::new_spanned(
                        &lit_str,
                        format!("failed to canonicalize path: {e}"),
                    )
                    .to_compile_error()
                    .into();
                }
            };
            let content = match std::fs::read_to_string(&resolved) {
                Ok(c) => c,
                Err(e) => {
                    return syn::Error::new_spanned(
                        &lit_str,
                        format!("failed to read grammar file: {e}"),
                    )
                    .to_compile_error()
                    .into();
                }
            };
            grammar_paths.push(abs_path_str);
            grammar_sources.push(content);
        } else if attr.path().is_ident("grammar_inline") {
            let lit_str: syn::LitStr = match &attr.meta {
                syn::Meta::NameValue(nv) => {
                    if let syn::Expr::Lit(syn::ExprLit {
                        lit: syn::Lit::Str(s),
                        ..
                    }) = &nv.value
                    {
                        s.clone()
                    } else {
                        return syn::Error::new_spanned(
                            &nv.value,
                            "expected string literal for #[grammar_inline = \"...\"]",
                        )
                        .to_compile_error()
                        .into();
                    }
                }
                syn::Meta::List(list) => match list.parse_args::<syn::LitStr>() {
                    Ok(s) => s,
                    Err(e) => return e.to_compile_error().into(),
                },
                _ => {
                    return syn::Error::new_spanned(
                        attr,
                        "expected #[grammar_inline = \"...\"] or #[grammar_inline(\"...\")]",
                    )
                    .to_compile_error()
                    .into();
                }
            };
            grammar_sources.push(lit_str.value());
        }
    }

    if grammar_sources.is_empty() {
        return syn::Error::new(
            proc_macro2::Span::call_site(),
            "derived Parser requires at least one #[grammar = \"...\"] or #[grammar_inline = \"...\"] attribute",
        )
        .to_compile_error()
        .into();
    }

    let combined_src = grammar_sources.join("\n");
    let grammar = match pika::Grammar::compile(&combined_src) {
        Ok(g) => g,
        Err(e) => {
            return syn::Error::new(proc_macro2::Span::call_site(), e.to_string())
                .to_compile_error()
                .into();
        }
    };

    let const_grammar_name = quote::format_ident!("_PIKA_GRAMMAR_{}", name);
    let includes = grammar_paths.iter().map(|path| {
        quote! {
            const _: &str = include_str!(#path);
        }
    });

    let variant_defs = grammar.rule_names().iter().enumerate().map(|(i, r)| {
        let raw = r.trim_start_matches("r#");
        let ident = syn::parse_str::<syn::Ident>(&format!("r#{}", raw)).unwrap();
        let idx = syn::Index::from(i);
        quote! {
            #ident = #idx,
        }
    });

    let all_rules_list = grammar.rule_names().iter().map(|r| {
        let raw = r.trim_start_matches("r#");
        let ident = syn::parse_str::<syn::Ident>(&format!("r#{}", raw)).unwrap();
        quote! {
            Rule::#ident,
        }
    });

    let match_names = grammar.rule_names().iter().map(|r| {
        let raw = r.trim_start_matches("r#");
        let ident = syn::parse_str::<syn::Ident>(&format!("r#{}", raw)).unwrap();
        quote! {
            Rule::#ident => #raw,
        }
    });

    let expanded = quote! {
        #(#includes)*

        #[allow(non_upper_case_globals)]
        const #const_grammar_name: &str = #combined_src;

        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        #[allow(dead_code, non_camel_case_types, clippy::upper_case_acronyms)]
        pub enum Rule {
            #(#variant_defs)*
        }

        impl Rule {
            pub fn all_rules() -> &'static [Rule] {
                &[ #(#all_rules_list)* ]
            }

            pub fn name(self) -> &'static str {
                match self {
                    #(#match_names)*
                }
            }

            #[inline]
            pub fn index(self) -> usize {
                self as usize
            }

            #[inline]
            pub fn from_index(i: usize) -> Rule {
                Self::all_rules()[i]
            }
        }

        impl ::pika::Parser<Rule> for #name {
            fn parse(rule: Rule, input: &str) -> ::std::result::Result<::pika::Pairs<'_, Rule>, ::pika::error::Error<Rule>> {
                static GRAMMAR: ::std::sync::LazyLock<::pika::Grammar> = ::std::sync::LazyLock::new(|| {
                    ::pika::Grammar::compile(#const_grammar_name)
                        .expect("grammar validated at compile time")
                });
                ::pika::parse_pairs(&GRAMMAR, rule.index(), input, Rule::from_index)
            }
        }

        impl #name {
            pub fn parse_table<'i>(rule: Rule, input: &'i str) -> ::pika::ParseTable<'static, 'i, Rule> {
                static GRAMMAR: ::std::sync::LazyLock<::pika::Grammar> = ::std::sync::LazyLock::new(|| {
                    ::pika::Grammar::compile(#const_grammar_name)
                        .expect("grammar validated at compile time")
                });
                ::pika::parse_table(&GRAMMAR, rule.index(), input, Rule::from_index)
            }
        }
    };

    expanded.into()
}
