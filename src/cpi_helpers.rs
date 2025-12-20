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
    if with_signer && signer_seeds.is_some() {
        let seeds = signer_seeds.unwrap();
        let seeds_code: Vec<String> = seeds.iter().map(|s| format!("        {},", s)).collect();
        format!(
            r#"// Token transfer with PDA signer
    Transfer {{
        from: {}.key(),
        to: {}.key(),
        authority: {}.key(),
    }}.invoke_signed(
        &[{}.clone(), {}.clone(), {}.clone()],
        &[&[
{}
        ]],
    )?;
    // Transfer amount: {}"#,
            from_account, to_account, authority,
            from_account, to_account, authority,
            seeds_code.join("\n"),
            amount
        )
    } else {
        format!(
            r#"// Token transfer
    Transfer {{
        from: {}.key(),
        to: {}.key(),
        authority: {}.key(),
    }}.invoke(&[{}.clone(), {}.clone(), {}.clone()])?;
    // Transfer amount: {}"#,
            from_account, to_account, authority,
            from_account, to_account, authority,
            amount
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
    if with_signer && signer_seeds.is_some() {
        let seeds = signer_seeds.unwrap();
        let seeds_code: Vec<String> = seeds.iter().map(|s| format!("        {},", s)).collect();
        format!(
            r#"// Mint tokens with PDA signer
    MintTo {{
        mint: {}.key(),
        account: {}.key(),
        mint_authority: {}.key(),
    }}.invoke_signed(
        &[{}.clone(), {}.clone(), {}.clone()],
        &[&[
{}
        ]],
    )?;
    // Mint amount: {}"#,
            mint_account, to_account, authority,
            mint_account, to_account, authority,
            seeds_code.join("\n"),
            amount
        )
    } else {
        format!(
            r#"// Mint tokens
    MintTo {{
        mint: {}.key(),
        account: {}.key(),
        mint_authority: {}.key(),
    }}.invoke(&[{}.clone(), {}.clone(), {}.clone()])?;
    // Mint amount: {}"#,
            mint_account, to_account, authority,
            mint_account, to_account, authority,
            amount
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
        account: {}.key(),
        mint: {}.key(),
        authority: {}.key(),
    }}.invoke(&[{}.clone(), {}.clone(), {}.clone()])?;
    // Burn amount: {}"#,
        from_account, mint_account, authority,
        from_account, mint_account, authority,
        amount
    )
}

/// Generate Pinocchio SOL transfer
pub fn sol_transfer_cpi(
    from_account: &str,
    to_account: &str,
    amount: &str,
) -> String {
    format!(
        r#"// SOL transfer
    **{}.try_borrow_mut_lamports()? -= {};
    **{}.try_borrow_mut_lamports()? += {};"#,
        from_account, amount,
        to_account, amount
    )
}

/// Common patterns for Pinocchio state access
pub fn state_read(state_type: &str, account: &str) -> String {
    format!(
        r#"let {} = {}::from_account_info({})?;"#,
        to_snake_case(state_type),
        state_type,
        account
    )
}

pub fn state_write(state_type: &str, account: &str) -> String {
    format!(
        r#"let {} = {}::from_account_info_mut({})?;"#,
        to_snake_case(state_type),
        state_type,
        account
    )
}

fn to_snake_case(s: &str) -> String {
    let mut result = String::new();
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() {
            if i > 0 {
                result.push('_');
            }
            result.push(c.to_lowercase().next().unwrap());
        } else {
            result.push(c);
        }
    }
    result
}
