#[cfg(not(feature = "library"))]
use cw_std::entry_point;
use {
    anyhow::bail,
    cw_std::{
        cw_serde, Addr, BankQuery, BankQueryResponse, Bound, Coin, Coins, ExecuteCtx,
        InstantiateCtx, Map, Order, QueryCtx, ReceiveCtx, Response, StdResult, Storage,
        TransferCtx, TransferMsg, Uint128,
    },
    std::collections::{HashMap, HashSet},
};

// (address, denom) => balance
const BALANCES: Map<(&Addr, &str), Uint128> = Map::new("b");

// denom => supply
const SUPPLIES: Map<&str, Uint128> = Map::new("s");

// how many items to return in a paginated query by default
const DEFAULT_PAGE_LIMIT: u32 = 30;

#[cw_serde]
pub struct InstantiateMsg {
    pub initial_balances: Vec<Balance>,
}

#[cw_serde]
pub struct Balance {
    pub address: Addr,
    pub coins:   Coins,
}

#[cw_serde]
pub enum ExecuteMsg {
    Mint {
        to:     Addr,
        denom:  String,
        amount: Uint128,
    },
    Burn {
        from:   Addr,
        denom:  String,
        amount: Uint128,
    },
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(ctx: InstantiateCtx, msg: InstantiateMsg) -> anyhow::Result<Response> {
    // need to make sure there are no duplicate address in initial balances.
    // we don't need to dedup denoms however. if there's duplicate denoms, the
    // deserialization setup should have already thrown an error.
    let mut seen_addrs = HashSet::new();
    let mut supplies = HashMap::new();

    for Balance { address, coins } in msg.initial_balances {
        if seen_addrs.contains(&address) {
            bail!("Duplicate address in initial balances");
        }

        for coin in coins {
            BALANCES.save(ctx.store, (&address, &coin.denom), &coin.amount)?;
            accumulate_supply(&mut supplies, &coin.denom, coin.amount)?;
        }

        seen_addrs.insert(address);
    }

    for (denom, amount) in supplies {
        SUPPLIES.save(ctx.store, &denom, &amount)?;
    }

    Ok(Response::new())
}

// just a helper function for use during instantiation
// not to be confused with `increase_supply` also found in this contract
fn accumulate_supply(
    supplies: &mut HashMap<String, Uint128>,
    denom:    &str,
    by:       Uint128,
) -> anyhow::Result<()> {
    let Some(supply) = supplies.get_mut(denom) else {
        supplies.insert(denom.into(), by);
        return Ok(());
    };

    *supply = supply.checked_add(by)?;

    Ok(())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn transfer(ctx: TransferCtx, msg: TransferMsg) -> StdResult<Response> {
    for coin in &msg.coins {
        decrease_balance(ctx.store, &msg.from, coin.denom, *coin.amount)?;
        increase_balance(ctx.store, &msg.to, coin.denom, *coin.amount)?;
    }

    Ok(Response::new()
        .add_attribute("method", "send")
        .add_attribute("from", msg.from)
        .add_attribute("to", msg.to)
        .add_attribute("coins", msg.coins.to_string()))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn receive(_ctx: ReceiveCtx) -> anyhow::Result<Response> {
    // we do not expect anyone to send any fund to this contract.
    // throw an error to revert the transfer.
    bail!("do not send funds to this contract");
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(ctx: ExecuteCtx, msg: ExecuteMsg) -> anyhow::Result<Response> {
    match msg {
        ExecuteMsg::Mint {
            to,
            denom,
            amount,
        } => mint(ctx, to, denom, amount),
        ExecuteMsg::Burn {
            from,
            denom,
            amount,
        } => burn(ctx, from, denom, amount),
    }
}

// NOTE: we haven't implement gatekeeping for minting/burning yet. for now
// anyone can mint any denom to any account, or burn any token from any account.
pub fn mint(
    ctx:    ExecuteCtx,
    to:     Addr,
    denom:  String,
    amount: Uint128,
) -> anyhow::Result<Response> {
    increase_supply(ctx.store, &denom, amount)?;
    increase_balance(ctx.store, &to, &denom, amount)?;

    Ok(Response::new()
        .add_attribute("method", "mint")
        .add_attribute("to", to)
        .add_attribute("denom", denom)
        .add_attribute("amount", amount))
}

// NOTE: we haven't implement gatekeeping for minting/burning yet. for now
// anyone can mint any denom to any account, or burn any token from any account.
pub fn burn(
    ctx:    ExecuteCtx,
    from:   Addr,
    denom:  String,
    amount: Uint128,
) -> anyhow::Result<Response> {
    decrease_supply(ctx.store, &denom, amount)?;
    decrease_balance(ctx.store, &from, &denom, amount)?;

    Ok(Response::new()
        .add_attribute("method", "burn")
        .add_attribute("from", from)
        .add_attribute("denom", denom)
        .add_attribute("amount", amount))
}

/// Increase the total supply of a token by the given amount.
/// Return the total supply value after the increase.
fn increase_supply(
    store:  &mut dyn Storage,
    denom:  &str,
    amount: Uint128,
) -> StdResult<Option<Uint128>> {
    SUPPLIES.update(store, denom, |supply| {
        let supply = supply.unwrap_or_default().checked_add(amount)?;
        Ok(Some(supply))
    })
}

/// Decrease the total supply of a token by the given amount.
/// Return the total supply value after the decrease.
fn decrease_supply(
    store:  &mut dyn Storage,
    denom:  &str,
    amount: Uint128,
) -> StdResult<Option<Uint128>> {
    SUPPLIES.update(store, denom, |supply| {
        let supply = supply.unwrap_or_default().checked_sub(amount)?;
        // if supply is reduced to zero, delete it, to save disk space
        if supply.is_zero() {
            Ok(None)
        } else {
            Ok(Some(supply))
        }
    })
}

/// Increase an account's balance of a token by the given amount.
/// Return the balance value after the increase.
fn increase_balance(
    store:   &mut dyn Storage,
    address: &Addr,
    denom:   &str,
    amount:  Uint128,
) -> StdResult<Option<Uint128>> {
    BALANCES.update(store, (address, denom), |balance| {
        let balance = balance.unwrap_or_default().checked_add(amount)?;
        Ok(Some(balance))
    })
}

/// Decrease an account's balance of a token by the given amount.
/// Return the balance value after the decrease.
fn decrease_balance(
    store:   &mut dyn Storage,
    address: &Addr,
    denom:   &str,
    amount:  Uint128,
) -> StdResult<Option<Uint128>> {
    BALANCES.update(store, (address, denom), |balance| {
        let balance = balance.unwrap_or_default().checked_sub(amount)?;
        // if balance is reduced to zero, delete it, to save disk space
        if balance.is_zero() {
            Ok(None)
        } else {
            Ok(Some(balance))
        }
    })
}

// Note to developers who wish to implement their own bank contracts:
// The query response MUST matches exactly the request. E.g. if the request is
// BankQuery::Balance, the response must be BankQueryResponse::Balance.
// It cannot be any other enum variant. Otherwise the chain may panic and halt.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query_bank(ctx: QueryCtx, msg: BankQuery) -> StdResult<BankQueryResponse> {
    match msg {
        BankQuery::Balance {
            address,
            denom,
        } => query_balance(ctx, address, denom).map(BankQueryResponse::Balance),
        BankQuery::Balances {
            address,
            start_after,
            limit,
        } => query_balances(ctx, address, start_after, limit).map(BankQueryResponse::Balances),
        BankQuery::Supply {
            denom,
        } => query_supply(ctx, denom).map(BankQueryResponse::Supply),
        BankQuery::Supplies {
            start_after,
            limit,
        } => query_supplies(ctx, start_after, limit).map(BankQueryResponse::Supplies),
    }
}

pub fn query_balance(ctx: QueryCtx, address: Addr, denom: String) -> StdResult<Coin> {
    let maybe_amount = BALANCES.may_load(ctx.store, (&address, &denom))?;
    Ok(Coin {
        denom,
        amount: maybe_amount.unwrap_or(Uint128::ZERO),
    })
}

pub fn query_balances(
    ctx:         QueryCtx,
    address:     Addr,
    start_after: Option<String>,
    limit:       Option<u32>,
) -> StdResult<Coins> {
    let start = start_after.as_ref().map(|denom| Bound::Exclusive(denom.as_str()));
    let limit = limit.unwrap_or(DEFAULT_PAGE_LIMIT) as usize;
    let mut iter = BALANCES
        .prefix(&address)
        .range(ctx.store, start, None, Order::Ascending)
        .take(limit);
    Coins::from_iter_unchecked(&mut iter)
}

pub fn query_supply(ctx: QueryCtx, denom: String) -> StdResult<Coin> {
    let maybe_supply = SUPPLIES.may_load(ctx.store, &denom)?;
    Ok(Coin {
        denom,
        amount: maybe_supply.unwrap_or(Uint128::ZERO),
    })
}

pub fn query_supplies(
    ctx:         QueryCtx,
    start_after: Option<String>,
    limit:       Option<u32>,
) -> StdResult<Coins> {
    let start = start_after.as_ref().map(|denom| Bound::Exclusive(denom.as_str()));
    let limit = limit.unwrap_or(DEFAULT_PAGE_LIMIT) as usize;
    let mut iter = SUPPLIES
        .range(ctx.store, start, None, Order::Ascending)
        .take(limit);
    Coins::from_iter_unchecked(&mut iter)
}
