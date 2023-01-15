//! improve readability of generated code

/// reformats the generated code to improve readability.
pub(crate) fn format(tables: proc_macro2::TokenStream) -> Result<String, syn::Error> {
    // if this is not valid code just pass it through directly, and then we
    // can see the compiler errors
    let source_str = match rustfmt_wrapper::rustfmt(&tables) {
        Ok(s) => s,
        Err(_) => return Ok(tables.to_string()),
    };
    // convert doc comment attributes into normal doc comments
    let doc_comments = regex::Regex::new(r##"#\[doc = r?#?"(.*)"#?\]"##).unwrap();
    let source_str = doc_comments.replace_all(&source_str, "///$1");
    let newlines_before_docs = regex::Regex::new(r#"([;\}])\r?\n( *)(///|pub|impl|#)"#).unwrap();
    let source_str = newlines_before_docs.replace_all(&source_str, "$1\n\n$2$3");

    // add newlines after top-level items
    let re2 = regex::Regex::new(r"\r?\n\}").unwrap();
    let source_str = re2.replace_all(&source_str, "\n}\n\n");
    Ok(rustfmt_wrapper::rustfmt(source_str).unwrap())
}
