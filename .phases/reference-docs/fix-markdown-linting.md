# Task: Fix Markdown Linting Errors

## Objective

Fix all markdown linting errors in the jsonb_ivm repository to meet strict quality standards.

## Context

The repository has markdown linting enabled via pre-commit hooks with strict rules. Several files have linting errors that need to be fixed manually.

## Files to Fix

Based on the linting output, fix these files:

1. `docs/troubleshooting.md` - 35 errors
2. `docs/implementation/implementation-success.md` - 10 errors

## Linting Rules to Follow

### MD031: Fenced code blocks must be surrounded by blank lines

**Wrong:**
    Some text here
    ```bash
    code here
    ```
    Next text

**Correct:**
    Some text here

    ```bash
    code here
    ```

    Next text

### MD040: Fenced code blocks must have a language specified

**Wrong:**
    ```
    some code
    ```

**Correct:**
    ```text
    some code
    ```

Or use specific language: `bash`, `sql`, `rust`, `json`, etc.

### MD022: Headings must be surrounded by blank lines

**Wrong:**
    Some text
    ### Heading
    Content

**Correct:**
    Some text

    ### Heading

    Content

### MD036: Don't use emphasis for headings

**Wrong:**
    **Option 1: Do something**

**Correct:**
    ### Option 1: Do something

Or if it must stay as emphasis, convert to proper heading level.

### MD012: No multiple consecutive blank lines

**Wrong:**
    Text here
    [multiple blank lines here]
    More text

**Correct:**
    Text here

    More text

## Implementation Steps

1. **Read `docs/troubleshooting.md`** completely
2. **Fix all 35 MD031 errors**: Add blank lines before and after ALL code fences
3. **Fix 2 MD040 errors**: Add language specifier to bare ``` fences (use `text` if unknown)
4. **Fix 3 MD036 errors**: Convert emphasis headings to proper headings (#### or ###)
5. **Save the file**

6. **Read `docs/implementation/implementation-success.md`** completely
7. **Fix all MD031 errors**: Add blank lines around code fences
8. **Fix MD022 error**: Add blank line before "### Build Output" heading
9. **Fix MD040 error**: Add language to bare code fence (probably `text` or `bash`)
10. **Save the file**

## Verification

After fixing all files, the pre-commit hook `markdownlint-cli2` should pass without errors.

## Acceptance Criteria

- [ ] All code fences have blank lines before and after them
- [ ] All code fences have a language specifier
- [ ] All headings have blank lines before and after them
- [ ] No emphasis used as headings (convert to proper heading levels)
- [ ] No multiple consecutive blank lines
- [ ] Pre-commit hooks pass: `pre-commit run markdownlint-cli2 --all-files`

## DO NOT

- Do not change the content or meaning of the documentation
- Do not rewrite sections
- Do not add or remove information
- Only fix formatting to meet linting rules
- Do not use scripts or automation tools
- Fix manually using Read and Edit tools only

## Example Workflow

    # Read file
    Read docs/troubleshooting.md

    # Find first code fence without surrounding blank lines (around line 50)
    # Edit to add blank lines

    # Repeat for all errors

    # Verify
    pre-commit run markdownlint-cli2 --all-files

## Expected Output

When complete:

    markdownlint-cli2........................................................Passed

All markdown files should pass strict linting with zero errors.
