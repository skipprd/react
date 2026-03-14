/// Canonical patch contracts used across prompts/tools.
///
/// Goal: keep the LLM-facing contract consistent everywhere so patch JSON shapes
/// don't drift between tools (and so strict serde parsing doesn't fail).
pub fn llm_patch_response_contract() -> String {
    format!(
        r#"Patch JSON schema (MUST follow exactly; structs are strict):
- Return ONE JSON object with optional notes and EXACTLY ONE patch payload.
- You MUST return `path` and it MUST equal expected_rel_path.
- You MUST return `patch_text` as Cursor/Aider-style hunks-only unified diff:
  - Starts with '@@'
  - Contains only hunks with -/+ lines (no git file headers like ---/+++ and no diff --git preamble)
  - Hunk headers MUST be Cursor/Aider style: '@@ ... @@' (no line numbers; never '@@ -a,b +c,d @@')
- The patch MUST modify ONLY expected_rel_path.
Output schema:
{{
  "notes": ["..."],
  "path": "<expected_rel_path>",
  "patch_text": "@@ ...\n- old\n+ new\n"
}}
Good args example:
{}"#,
        crate::patch_contract::single_file_patch_good_example_json()
    )
}

pub fn file_patch_contract() -> String {
    use crate::tool_ops::FileOpKind;

    let ops_list = [FileOpKind::Patch, FileOpKind::Write, FileOpKind::Rm, FileOpKind::Mv];
    let ops_quoted = crate::tool_ops::ops_quoted_label(&ops_list);

    format!(
        r#"file operations contract (MUST follow exactly):
- args.op MUST be one of: {ops_quoted}

op="{patch}":
- Hard cutover: Cursor/Aider hunks-only unified diff ONLY.
- args: {{op:"{patch}", {patch_sig}}}
  - args.path MUST be the single file to mutate.
  - patch_text MUST start with '@@' and MUST NOT include git file headers (---/+++), diff --git preamble, or diffy-style headers ('--- original' / '+++ modified').
  - Hunk headers MUST be Cursor/Aider style: '@@ ... @@' (no line numbers; never '@@ -a,b +c,d @@').
Example args:
{patch_example}

op="{write}":
- Full file overwrite. Use when patches fail or the file needs to be rewritten from scratch.
- args: {{op:"{write}", {write_sig}}}
  - args.path MUST be the single file to write.
  - args.content is the complete file content (replaces everything).
Example args:
{{"op":"{write}","path":"models/staging/stg_example.sql","content":"{{{{ config(materialized='view') }}}}\n\nselect * from {{{{ source('raw','events') }}}}"}}

op="{rm}":
- args: {{{rm_sig}}}
- If expected_sha256 is provided and the file exists, it MUST match the current file sha256.
Example args:
{{"op":"{rm}","path":"models/staging/staging.sql"}}

op="{mv}":
- args: {{{mv_sig}}}
- Destination MUST NOT already exist (no implicit overwrite).
- If expected_sha256 is provided, it MUST match the current source file sha256.
Example args:
{{"op":"{mv}","from":"models/staging/foo.sql","to":"models/staging/stg_test_raw_raw_customers.sql"}}"#,
        patch = FileOpKind::Patch.as_str(),
        patch_sig = FileOpKind::Patch.args_signature(),
        patch_example = crate::patch_contract::single_file_patch_good_example_json(),
        write = FileOpKind::Write.as_str(),
        write_sig = FileOpKind::Write.args_signature(),
        rm = FileOpKind::Rm.as_str(),
        rm_sig = FileOpKind::Rm.args_signature(),
        mv = FileOpKind::Mv.as_str(),
        mv_sig = FileOpKind::Mv.args_signature(),
    )
}
