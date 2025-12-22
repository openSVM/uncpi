//! Intermediate Representation for both Anchor and Pinocchio programs

use serde::{Deserialize, Serialize};

// ============================================================================
// Anchor IR (parsed from source)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnchorProgram {
    pub name: String,
    pub program_id: Option<String>,
    pub instructions: Vec<AnchorInstruction>,
    pub account_structs: Vec<AnchorAccountStruct>,
    pub state_structs: Vec<AnchorStateStruct>,
    pub errors: Vec<AnchorError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnchorInstruction {
    pub name: String,
    pub accounts_struct: String,
    pub args: Vec<InstructionArg>,
    pub body: String, // Raw function body
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstructionArg {
    pub name: String,
    pub ty: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnchorAccountStruct {
    pub name: String,
    pub instruction_args: Vec<InstructionArg>, // From #[instruction(...)]
    pub accounts: Vec<AnchorAccount>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnchorAccount {
    pub name: String,
    pub ty: AccountType,
    pub constraints: Vec<AccountConstraint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AccountType {
    Account { inner: String },       // Account<'info, T>
    Signer,                          // Signer<'info>
    SystemAccount,                   // SystemAccount<'info>
    UncheckedAccount,                // UncheckedAccount<'info>
    Program { inner: String },       // Program<'info, T>
    Sysvar { inner: String },        // Sysvar<'info, T>
    TokenAccount,                    // anchor_spl::token::TokenAccount
    Mint,                            // anchor_spl::token::Mint
    Box { inner: Box<AccountType> }, // Box<Account<...>>
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AccountConstraint {
    Mut,
    Init {
        payer: String,
        space: String,
    },
    InitIfNeeded {
        payer: String,
        space: String,
    },
    Seeds(Vec<String>),
    Bump(Option<String>), // None = canonical bump, Some(x) = x.bump
    TokenMint(String),
    TokenAuthority(String),
    MintDecimals(u8),
    MintAuthority(String),
    Constraint {
        expr: String,
        error: Option<String>,
    },
    HasOne {
        field: String,
        error: Option<String>,
    },
    Address(String),
    Close(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnchorStateStruct {
    pub name: String,
    pub fields: Vec<StateField>,
    pub has_init_space: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateField {
    pub name: String,
    pub ty: String,
    pub max_len: Option<usize>, // For String fields with #[max_len(N)]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnchorError {
    pub name: String,
    pub code: Option<u32>,
    pub msg: String,
}

// ============================================================================
// Analysis Results
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgramAnalysis {
    pub pdas: Vec<PdaInfo>,
    pub cpi_calls: Vec<CpiCall>,
    pub account_sizes: Vec<AccountSize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PdaInfo {
    pub account_name: String,
    pub seeds: Vec<String>,
    pub bump_source: Option<String>, // Where bump comes from
    pub program_id: String,          // Usually "program_id"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpiCall {
    pub target_program: String,
    pub instruction: String,
    pub accounts: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountSize {
    pub struct_name: String,
    pub size: usize,
    pub fields: Vec<(String, usize)>,
}

// ============================================================================
// Pinocchio IR (to be emitted)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PinocchioProgram {
    pub name: String,
    pub program_id: Option<String>,
    pub config: PinocchioConfig,
    pub instructions: Vec<PinocchioInstruction>,
    pub state_structs: Vec<PinocchioState>,
    pub errors: Vec<PinocchioError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PinocchioConfig {
    pub no_alloc: bool,
    pub lazy_entrypoint: bool,
    pub anchor_compat: bool, // Use 8-byte discriminators like Anchor
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PinocchioInstruction {
    pub name: String,
    pub discriminator: Vec<u8>,
    pub accounts: Vec<PinocchioAccount>,
    pub args: Vec<InstructionArg>,
    pub validations: Vec<Validation>,
    pub body: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PinocchioAccount {
    pub name: String,
    pub index: usize,
    pub is_signer: bool,
    pub is_writable: bool,
    pub is_pda: bool,
    pub pda_seeds: Option<Vec<String>>,
    pub is_init: bool,
    pub token_mint: Option<String>,      // For init token accounts
    pub token_authority: Option<String>, // For init token accounts
    pub init_payer: Option<String>,      // Who pays for initialization
    pub state_type: Option<String>,      // The state struct type for this account (e.g., "Pool", "Escrow")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Validation {
    IsSigner {
        account_idx: usize,
    },
    IsWritable {
        account_idx: usize,
    },
    PdaCheck {
        account_idx: usize,
        seeds: Vec<String>,
        bump: Option<String>,
    },
    OwnerCheck {
        account_idx: usize,
        owner: String,
    },
    KeyEquals {
        account_idx: usize,
        expected: String,
    },
    Custom {
        code: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PinocchioState {
    pub name: String,
    pub size: usize,
    pub fields: Vec<PinocchioField>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PinocchioField {
    pub name: String,
    pub ty: String,
    pub size: usize,
    pub offset: usize,
    pub max_len: Option<usize>, // For String fields with #[max_len(N)]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PinocchioError {
    pub name: String,
    pub code: u32,
    pub msg: String,
}
