# VecDeque Transformation Design Document

**Feature**: Transform `VecDeque<T>` to fixed arrays with manual index management
**Target**: uncpi v0.4.0  
**Status**: Design Complete, Awaiting Implementation

## Overview

Raydium CLMM uses `VecDeque<TickArrayState>` for multi-tick crossing during swaps. This feature transforms VecDeque to fixed-size arrays with head/tail pointers.

## Transformation Strategy

### Input
```rust
let mut tick_arrays = VecDeque::with_capacity(5);
tick_arrays.push_back(first_array);
tick_arrays.push_front(prev_array);
let current = tick_arrays.pop_front();
```

### Output
```rust
const MAX_TICK_ARRAYS: usize = 5;
let mut tick_arrays: [Option<TickArrayState>; MAX_TICK_ARRAYS] = [None; MAX_TICK_ARRAYS];
let mut tick_arrays_head: usize = 0;
let mut tick_arrays_tail: usize = 0;

// push_back
tick_arrays[tick_arrays_tail] = Some(first_array);
tick_arrays_tail = (tick_arrays_tail + 1) % MAX_TICK_ARRAYS;

// push_front  
tick_arrays_head = (tick_arrays_head + MAX_TICK_ARRAYS - 1) % MAX_TICK_ARRAYS;
tick_arrays[tick_arrays_head] = Some(prev_array);

// pop_front
let current = tick_arrays[tick_arrays_head].take();
tick_arrays_head = (tick_arrays_head + 1) % MAX_TICK_ARRAYS;
```

## Implementation Tasks

- [ ] Detect VecDeque usage and capacity
- [ ] Transform to circular array with head/tail
- [ ] Transform all operations (push_back, push_front, pop_front, pop_back)
- [ ] Handle iter() and len()
- [ ] Add overflow checking
- [ ] Create tests with CLMM swap example

## Key Files
- `src/collections.rs` - VecDeque transformation logic
- `src/transformer/mod.rs` - Apply transformations
- `docs/examples/clmm_swap.rs` - Example usage

*See ADVANCED-FEATURES-PLAN.md for detailed implementation guide*
