# Advanced CPI Patterns Design Document

**Feature**: Support complex CPI patterns including remaining accounts and dynamic dispatch
**Target**: uncpi v0.4.0
**Status**: Design Complete, Awaiting Implementation

## Overview

Advanced CPIs include remaining accounts, nested CPIs, and dynamic account selection. This feature enhances CPI transformation to handle these patterns.

## Patterns to Support

### 1. Remaining Accounts
```rust
// Anchor
let remaining = ctx.remaining_accounts;
for account in remaining {
    process_account(account)?;
}

// Pinocchio
const EXPECTED_ACCOUNTS: usize = 5;
for i in EXPECTED_ACCOUNTS..accounts.len() {
    let account = &accounts[i];
    process_account(account)?;
}
```

### 2. Dynamic Account Selection
```rust
// Anchor
let target = if use_a { &ctx.accounts.account_a } else { &ctx.accounts.account_b };

// Pinocchio
let target = if use_a { &accounts[ACCOUNT_A] } else { &accounts[ACCOUNT_B] };
```

### 3. Interface Programs
```rust
// Anchor
pub token_program: Interface<'info, TokenInterface>,

// Pinocchio
pub token_program: &AccountInfo,  // Runtime dispatch
```

## Implementation Tasks

- [ ] Detect remaining_accounts usage
- [ ] Calculate expected account count
- [ ] Transform dynamic account access
- [ ] Handle Interface types
- [ ] Support nested CPI contexts
- [ ] Add tests for each pattern

## Key Files
- `src/cpi_helpers.rs` - CPI pattern detection
- `src/transformer/mod.rs` - CPI transformations
- `docs/examples/advanced_cpi.rs` - Example usage

*See ADVANCED-FEATURES-PLAN.md for detailed implementation guide*
