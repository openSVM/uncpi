//! CPI Helper Code Generation for Pinocchio
//!
//! Generates Pinocchio-style CPI calls from Anchor patterns

/// Generate a Pinocchio token transfer CPI call
pub fn token_transfer_cpi(
    from_account: &str,
    to_account: &str,
    authority: &str,
    amount: &str,
    with_signer: bool,
    signer_seeds: Option<&[&str]>,
) -> String {
    if let (true, Some(seeds)) = (with_signer, signer_seeds) {
        let seeds_code: Vec<String> = seeds.iter().map(|s| format!("        {},", s)).collect();
        format!(
            r#"// Token transfer with PDA signer
    Transfer {{
        from: {},
        to: {},
        authority: {},
        amount: {},
    }}.invoke_signed(
        &[&[
{}
        ]],
    )?;
"#,
            from_account,
            to_account,
            authority,
            amount,
            seeds_code.join("\n")
        )
    } else {
        format!(
            r#"// Token transfer
    Transfer {{
        from: {},
        to: {},
        authority: {},
        amount: {},
    }}.invoke()?;
"#,
            from_account, to_account, authority, amount
        )
    }
}

/// Generate a Pinocchio token mint CPI call
pub fn token_mint_to_cpi(
    mint_account: &str,
    to_account: &str,
    authority: &str,
    amount: &str,
    with_signer: bool,
    signer_seeds: Option<&[&str]>,
) -> String {
    if let (true, Some(seeds)) = (with_signer, signer_seeds) {
        let seeds_code: Vec<String> = seeds.iter().map(|s| format!("        {},", s)).collect();
        format!(
            r#"// Mint tokens with PDA signer
    MintTo {{
        mint: {},
        account: {},
        mint_authority: {},
        amount: {},
    }}.invoke_signed(
        &[&[
{}
        ]],
    )?;
"#,
            mint_account,
            to_account,
            authority,
            amount,
            seeds_code.join("\n")
        )
    } else {
        format!(
            r#"// Mint tokens
    MintTo {{
        mint: {},
        account: {},
        mint_authority: {},
        amount: {},
    }}.invoke()?;
"#,
            mint_account, to_account, authority, amount
        )
    }
}

/// Generate a Pinocchio token burn CPI call
pub fn token_burn_cpi(
    mint_account: &str,
    from_account: &str,
    authority: &str,
    amount: &str,
) -> String {
    format!(
        r#"// Burn tokens
    Burn {{
        account: {},
        mint: {},
        authority: {},
        amount: {},
    }}.invoke()?;
"#,
        from_account, mint_account, authority, amount
    )
}

/// Generate Pinocchio SOL transfer (direct lamport manipulation)
/// Used when we want to generate inline SOL transfers instead of system_program CPI
/// This is the most gas-efficient way to transfer SOL in Pinocchio
pub fn sol_transfer_cpi(from_account: &str, to_account: &str, amount: &str) -> String {
    format!(
        r#"// SOL transfer
    **{}.try_borrow_mut_lamports()? -= {};
    **{}.try_borrow_mut_lamports()? += {};"#,
        from_account, amount, to_account, amount
    )
}

/// Generate state deserialization code (readonly)
/// Matches the pattern used in transformer: let {account}_state = {StateType}::from_account_info({account})?
pub fn state_deserialize_read(state_type: &str, account_name: &str) -> String {
    format!(
        "let {}_state = {}::from_account_info({})?;",
        account_name, state_type, account_name
    )
}

/// Generate state deserialization code (mutable)
/// Matches the pattern used in transformer: let {account}_state = {StateType}::from_account_info_mut({account})?
pub fn state_deserialize_write(state_type: &str, account_name: &str, needs_mut: bool) -> String {
    if needs_mut {
        format!(
            "let mut {}_state = {}::from_account_info_mut({})?;",
            account_name, state_type, account_name
        )
    } else {
        format!(
            "let {}_state = {}::from_account_info_mut({})?;",
            account_name, state_type, account_name
        )
    }
}
