use proc_macro::TokenStream;
use proc_macro2::Literal;
use quote::quote;
use syn::{parse_macro_input, LitStr, punctuated::Punctuated, Token};

struct ShaderArgs {
    path: LitStr,
    entry_points: Vec<LitStr>,
}

impl syn::parse::Parse for ShaderArgs {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let path: LitStr = input.parse()?;
        input.parse::<Token![,]>()?;
        let content;
        syn::bracketed!(content in input);
        let entry_points = Punctuated::<LitStr, Token![,]>::parse_terminated(&content)?
            .into_iter()
            .collect();
        Ok(Self { path, entry_points })
    }
}

#[proc_macro]
pub fn compile_shader(input: TokenStream) -> TokenStream {
    let args = parse_macro_input!(input as ShaderArgs);
    let path = args.path.value();

    let entry_points: Vec<String> = args.entry_points.iter().map(|l| l.value()).collect();
    let entry_refs: Vec<&str> = entry_points.iter().map(String::as_str).collect();

    let spirv = build_shader(&path, &entry_refs)
        .unwrap_or_else(|e| panic!("shader compile failed for {path}: {e}"));

    // vk::ShaderModuleCreateInfo::code wants &[u32]; slang gives raw bytes.
    let words: Vec<u32> = spirv
        .as_slice()
        .chunks_exact(4)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect();

    // resolve same way slang does (relative to process cwd, search path "."),
    // not relative to this source file, so the dep-tracking include lines up.
    let abs_path = std::env::current_dir().unwrap().join(&path);
    let abs_path_lit = Literal::string(&abs_path.to_string_lossy());

    quote! {
        {
            const _: &[u8] = include_bytes!(#abs_path_lit); // forces rebuild when shader source changes
            &[#(#words),*][..]
        }
    }
    .into()
}

fn build_shader(
    name: &str,
    entry_points: &[&str],
) -> Result<slang::Blob, Box<dyn std::error::Error + Send + Sync>> {
    let global_session = slang::GlobalSession::new().ok_or("Slang not found")?;
    let target_desc = slang::TargetDesc::default()
        .format(slang::CompileTarget::Spirv)
        .profile(global_session.find_profile("glsl_450"));
    let targets = [target_desc];
    let binding = slang::CompilerOptions::default().matrix_layout_column(true);
    let paths = [c".".as_ptr()];
    let session_desc = slang::SessionDesc::default()
        .targets(&targets)
        .search_paths(&paths)
        .options(&binding);
    let session = global_session.create_session(&session_desc).unwrap();
    let module = session.load_module(name)?;

    let mut components = entry_points
        .iter()
        .map(|&ep| {
            module
                .find_entry_point_by_name(ep)
                .ok_or(format!("entry point {} not found", ep))
                .unwrap()
                .into()
        })
        .collect::<Vec<slang::ComponentType>>();
    components.insert(0, module.into());

    let program = session.create_composite_component_type(&components)?;
    let linked = program.link()?;
    let code = linked.target_code(0)?;

    Ok(code)
}