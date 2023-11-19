use proc_macro::TokenStream;
use quote::{quote, ToTokens};
use std::ops::Deref;
use syn::{ItemFn, Pat::Ident, Type};

struct NapiFnArgs {
    ident: syn::Ident,
    ty: Type,
}

#[proc_macro_attribute]
pub fn napi(_attr: TokenStream, input: TokenStream) -> TokenStream {
    let ast = syn::parse::<ItemFn>(input).unwrap();

    // 函数名
    let name = &ast.sig.ident;

    let sig = &ast.sig;
    let params = &ast.sig.inputs;
    let result = &ast.sig.output;
    let fn_blocks = &ast.block;

    let ret_ty = match result {
        syn::ReturnType::Type(_,ty) => quote! { #ty },
        syn::ReturnType::Default => quote! { () },
    };

    let org_sig = quote! {
        #sig
    };

    let org_block = quote! {
        #fn_blocks
    };

    let args = params
        .iter()
        .filter_map(|arg| match arg {
            syn::FnArg::Typed(ref p) => {
                if let Ident(ref ident) = *p.pat {
                    let param_type = &p.ty;
                    Some(NapiFnArgs {
                        ident: ident.ident.clone(),
                        ty: p.ty.clone().deref().clone(),
                    })
                } else {
                    None
                }
            }
            syn::FnArg::Receiver(ref p) => None,
        })
        .collect::<Vec<NapiFnArgs>>();

    let arg_cnt = args.len();
    let js_args = args.iter().enumerate().map(|(index, &ref ident)| {
        let arg = syn::Ident::new(
            format!("arg_{}", index).as_str(),
            proc_macro2::Span::call_site(),
        );
        let ty = &ident.ty.clone();
        quote! {
            let #arg = <#ty as crate::value::NapiValue>::get_value_from_raw(env,args[#index]);
        }
    });
    let js_name = syn::Ident::new(
        format!("js_{}", name).as_str(),
        proc_macro2::Span::call_site(),
    );

    let run_args = args.iter().enumerate().map(|(index, ident)| {
        let arg = syn::Ident::new(
            format!("arg_{}", index).as_str(),
            proc_macro2::Span::call_site(),
        );
        quote! {
           #arg
        }
    });

    let expanded = quote! {
        use std::ptr;
        use std::ffi::CString;

        #org_sig
        #org_block

        unsafe extern "C" fn #js_name(
            env: sys::napi_env,
            callback: sys::napi_callback_info,
        ) -> sys::napi_value {
            unsafe {
                let mut args = [ptr::null_mut(); #arg_cnt];
                sys::napi_get_cb_info(
                    env,
                    callback,
                    &mut #arg_cnt,
                    args.as_mut_ptr(),
                    ptr::null_mut(),
                    ptr::null_mut(),
                );
                #(#js_args)*

                let ret = #name(#(#run_args),*);

                <#ret_ty as crate::value::NapiValue>::try_into_raw(env,ret)
            }
        }
        unsafe extern "C" fn napi_register_module_v1(
            env: sys::napi_env,
            exports: sys::napi_value,
        ) -> sys::napi_value {
            let name = CString::new("add").unwrap();
            let desc = [sys::napi_property_descriptor {
                utf8name: name.as_ptr().cast(),
                name: ptr::null_mut(),
                getter: None,
                setter: None,
                method: Some(#js_name),
                attributes: 0,
                value: ptr::null_mut(),
                data: ptr::null_mut(),
            }];
            sys::napi_define_properties(env, exports, desc.len(), desc.as_ptr());
            exports
        }
        #[ctor::ctor]
        fn init() {
            let name = CString::new("entry").unwrap();
            let mut modules = sys::napi_module {
                nm_version: 1,
                nm_filename: ptr::null_mut(),
                nm_flags: 0,
                nm_modname: name.as_ptr().cast(),
                nm_priv: ptr::null_mut() as *mut _,
                nm_register_func: Some(napi_register_module_v1),
                reserved: [ptr::null_mut() as *mut _; 4],
            };
            unsafe {
                sys::napi_module_register(&mut modules);
            }
        }
    };
    expanded.into()
}
