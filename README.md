# uncpi

[![Crates.io](https://img.shields.io/crates/v/uncpi.svg)](https://crates.io/crates/uncpi)
[![CI](https://github.com/openSVM/uncpi/workflows/CI/badge.svg)](https://github.com/openSVM/uncpi/actions)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](https://opensource.org/licenses/MIT)

> "ok unc, optimize my anchor uwu" (c) clanker

Transpile Anchor programs to Pinocchio for 85%+ binary size reduction.

## Why?

| Metric | Anchor | Pinocchio | Savings |
|--------|--------|-----------|---------|
| Binary Size | ~600-900 KB | ~50-100 KB | **85-90%** |
| Deploy Cost | ~10 SOL | ~1 SOL | **$1,100+** |
| Compute Units | ~300-600 CU | ~100-150 CU | **60-75%** |

## Install

### From crates.io (recommended)

```bash
cargo install uncpi
```

### From source

```bash
git clone https://github.com/openSVM/uncpi.git
cd uncpi
cargo install --path .
```

### Pre-built binaries

Download pre-built binaries for Linux, macOS, and Windows from the [releases page](https://github.com/openSVM/uncpi/releases).

## Usage

```bash
# Convert a program
uncpi programs/my_program/src/lib.rs -o programs/my_program_pino/

# With options
uncpi programs/my_program/src/lib.rs \
    --no-alloc \           # Use no_allocator! for max savings
    --inline-cpi \         # Inline CPI calls
    --lazy-entrypoint \    # Use lazy_program_entrypoint!
    -o output/
```

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                        uncpi Pipeline                           │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  ┌──────────┐    ┌──────────┐    ┌──────────┐    ┌──────────┐  │
│  │  Parse   │───▶│ Analyze  │───▶│Transform │───▶│  Emit    │  │
│  │  Anchor  │    │   AST    │    │   IR     │    │Pinocchio │  │
│  └──────────┘    └──────────┘    └──────────┘    └──────────┘  │
│       │               │               │               │         │
│       ▼               ▼               ▼               ▼         │
│  lib.rs         Account Graph    Pinocchio IR     lib.rs       │
│  (Anchor)       + Constraints    + Validation    (Pinocchio)   │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

## Conversion Examples

### Program Entry Point

**Anchor:**
```rust
#[program]
pub mod my_program {
    pub fn initialize(ctx: Context<Initialize>) -> Result<()> {
        // ...
    }
}
```

**Pinocchio:**
```rust
use pinocchio::{lazy_program_entrypoint, program_error::ProgramError};

lazy_program_entrypoint!(process_instruction);

fn process_instruction(
    program_id: &[u8; 32],
    accounts: &[[u8; 32]],
    instruction_data: &[u8],
) -> Result<(), ProgramError> {
    let discriminator = instruction_data[0];
    match discriminator {
        0 => initialize(accounts, &instruction_data[1..]),
        _ => Err(ProgramError::InvalidInstructionData),
    }
}
```

### Account Validation

**Anchor:**
```rust
#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(
        init,
        payer = authority,
        space = 8 + State::INIT_SPACE,
        seeds = [b"state"],
        bump
    )]
    pub state: Account<'info, State>,

    #[account(mut)]
    pub authority: Signer<'info>,

    pub system_program: Program<'info, System>,
}
```

**Pinocchio:**
```rust
fn initialize(accounts: &[AccountInfo], _data: &[u8]) -> Result<(), ProgramError> {
    let state_info = &accounts[0];
    let authority_info = &accounts[1];
    let system_program = &accounts[2];

    // Signer check
    if !authority_info.is_signer() {
        return Err(ProgramError::MissingRequiredSignature);
    }

    // PDA validation
    let (expected_pda, bump) = Pubkey::find_program_address(&[b"state"], program_id);
    if state_info.key() != &expected_pda {
        return Err(ProgramError::InvalidSeeds);
    }

    // Create account via CPI
    create_account(
        authority_info,
        state_info,
        8 + State::SIZE,
        program_id,
        system_program,
        &[b"state", &[bump]],
    )?;

    Ok(())
}
```

### State Structs

**Anchor:**
```rust
#[account]
#[derive(InitSpace)]
pub struct State {
    pub authority: Pubkey,
    pub total: u64,
    pub bump: u8,
}
```

**Pinocchio:**
```rust
#[repr(C)]
pub struct State {
    pub authority: [u8; 32],
    pub total: u64,
    pub bump: u8,
}

impl State {
    pub const SIZE: usize = 32 + 8 + 1;

    pub fn from_account_info(info: &AccountInfo) -> Result<&Self, ProgramError> {
        let data = info.try_borrow_data()?;
        if data.len() < 8 + Self::SIZE {
            return Err(ProgramError::InvalidAccountData);
        }
        Ok(unsafe { &*(data[8..].as_ptr() as *const Self) })
    }
}
```

## Constraints Mapping

| Anchor Constraint | Pinocchio Equivalent |
|-------------------|---------------------|
| `#[account(mut)]` | `info.is_writable()` check |
| `#[account(signer)]` | `info.is_signer()` check |
| `#[account(seeds = [...], bump)]` | `Pubkey::find_program_address()` |
| `#[account(constraint = expr @ Error)]` | Manual `if !expr { return Err(...) }` |
| `#[account(init, payer, space)]` | `create_account()` CPI |
| `#[account(token::mint, token::authority)]` | Token program CPI + validation |
| `#[account(close = target)]` | Transfer lamports + zero data |

## CPI Calls

**Anchor:**
```rust
let cpi_accounts = Transfer {
    from: ctx.accounts.from.to_account_info(),
    to: ctx.accounts.to.to_account_info(),
    authority: ctx.accounts.authority.to_account_info(),
};
token::transfer(CpiContext::new(token_program, cpi_accounts), amount)?;
```

**Pinocchio:**
```rust
use pinocchio_token::instructions::Transfer;

Transfer {
    from: from_info,
    to: to_info,
    authority: authority_info,
    amount,
}.invoke()?;
```

## Limitations

1. **No IDL generation** - Must maintain IDL manually
2. **Complex constraints** - Some runtime constraints need manual review
3. **Account iteration** - `remaining_accounts` patterns need manual handling
4. **Dynamic PDA seeds** - Runtime seeds need careful handling

## License

MIT
