use cosmwasm_std::{Addr, Decimal, Uint128};
use terra_multi_test::Executor;

use astroport::asset::{Asset, AssetInfo};
use astroport::pair_reserve::{
    ConfigResponse, ExecuteMsg, FlowParams, PoolParams, QueryMsg, UpdateFlowParams, UpdateParams,
};

use crate::test_utils::{mock_app, Helper};
use crate::test_utils::{AssetExt, AssetsExt};

#[cfg(test)]
mod test_utils;

#[test]
fn test_config_update() {
    let mut router = mock_app();
    let owner = Addr::unchecked("owner");
    let helper = Helper::init(&mut router, &owner);

    let mut params = UpdateParams {
        entry: Some(UpdateFlowParams {
            base_pool: Uint128::zero(),
            min_spread: 0,
            recovery_period: 0,
        }),
        exit: None,
    };

    let err = router
        .execute_contract(
            Addr::unchecked("anyone"),
            helper.pair.clone(),
            &ExecuteMsg::UpdatePoolParameters {
                params: params.clone(),
            },
            &[],
        )
        .unwrap_err();
    assert_eq!(&err.to_string(), "Unauthorized");

    let err = router
        .execute_contract(
            owner.clone(),
            helper.pair.clone(),
            &ExecuteMsg::UpdatePoolParameters {
                params: params.clone(),
            },
            &[],
        )
        .unwrap_err();
    assert_eq!(
        &err.to_string(),
        "Inflow validation error: base_pool cannot be zero"
    );

    params.entry = params.entry.map(|mut flow| {
        flow.base_pool = Uint128::from(1000u128);
        flow
    });

    let err = router
        .execute_contract(
            owner.clone(),
            helper.pair.clone(),
            &ExecuteMsg::UpdatePoolParameters {
                params: params.clone(),
            },
            &[],
        )
        .unwrap_err();
    assert_eq!(
        &err.to_string(),
        "Inflow validation error: Min spread must be within [1, 10000] limit"
    );

    params.entry = params.entry.map(|mut flow| {
        flow.min_spread = 500;
        flow
    });
    let err = router
        .execute_contract(
            owner.clone(),
            helper.pair.clone(),
            &ExecuteMsg::UpdatePoolParameters {
                params: params.clone(),
            },
            &[],
        )
        .unwrap_err();
    assert_eq!(
        &err.to_string(),
        "Inflow validation error: Recovery period cannot be zero"
    );

    params.entry = params.entry.map(|mut flow| {
        flow.recovery_period = 100;
        flow
    });
    router
        .execute_contract(
            owner.clone(),
            helper.pair.clone(),
            &ExecuteMsg::UpdatePoolParameters {
                params: params.clone(),
            },
            &[],
        )
        .unwrap();
    let config = helper.get_config(&mut router).unwrap();
    let need_params = PoolParams {
        entry: FlowParams {
            base_pool: Uint128::from(1000u128),
            min_spread: 500,
            recovery_period: 100,
            last_repl_block: 0,
            pool_delta: Decimal::zero(),
        },
        exit: FlowParams {
            base_pool: Uint128::from(100_000_000_000000u128),
            min_spread: 100,
            recovery_period: 100,
            last_repl_block: 0,
            pool_delta: Decimal::zero(),
        },
        oracles: helper.oracles.clone(),
    };
    assert_eq!(config.pool_params, need_params);
}

#[test]
fn test_liquidity_operations() {
    let mut router = mock_app();
    let owner = Addr::unchecked("owner");
    let helper = Helper::init(&mut router, &owner);

    let assets = helper.assets.with_balances(100, 0);
    let err = helper
        .provide_liquidity(&mut router, "user", assets.clone(), None)
        .unwrap_err();
    // user is not in the whitelist
    assert_eq!(&err.to_string(), "Unauthorized");

    helper
        .update_whitelist(&mut router, "owner", vec!["user"], vec![])
        .unwrap();
    let err = helper
        .provide_liquidity(&mut router, "user", assets.clone(), None)
        .unwrap_err();
    // User does not have enough coins
    assert_eq!(&err.to_string(), "Overflow: Cannot Sub with 0 and 100");

    helper.give_coins(&mut router, "user", &assets[0]);
    helper
        .provide_liquidity(&mut router, "user", assets, None)
        .unwrap();
    let lp_balance = helper
        .get_token_balance(&mut router, &helper.lp_token, "user")
        .unwrap();
    assert_eq!(lp_balance, 100u128);

    let assets = helper.assets.with_balances(50, 0);
    helper.give_coins(&mut router, "user", &assets[0]);
    helper
        .provide_liquidity(&mut router, "user", assets, Some("user2"))
        .unwrap();
    let lp_balance = helper
        .get_token_balance(&mut router, &helper.lp_token, "user")
        .unwrap();
    assert_eq!(lp_balance, 100u128);
    let lp_balance = helper
        .get_token_balance(&mut router, &helper.lp_token, "user2")
        .unwrap();
    assert_eq!(lp_balance, 50u128);

    let assets = [
        Asset {
            info: AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
            amount: Default::default(),
        },
        Asset {
            info: AssetInfo::NativeToken {
                denom: "uluna".to_string(),
            },
            amount: Default::default(),
        },
    ];
    let err = helper
        .provide_liquidity(&mut router, "user", assets, None)
        .unwrap_err();
    assert_eq!(
        &err.to_string(),
        "Generic error: Reserve pool accepts (native token, CW20 token) pairs only"
    );

    let assets = [
        Asset {
            info: AssetInfo::NativeToken {
                denom: "ibc/uusd".to_string(),
            },
            amount: Default::default(),
        },
        helper.assets[0].clone(),
    ];
    let err = helper
        .provide_liquidity(&mut router, "user", assets, None)
        .unwrap_err();
    assert_eq!(&err.to_string(), "Generic error: IBC tokens are forbidden");

    let assets = [
        Asset {
            info: AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
            amount: Default::default(),
        },
        Asset {
            info: AssetInfo::Token {
                contract_addr: helper.cw20_token.clone(),
            },
            amount: Default::default(),
        },
    ];
    let err = helper
        .provide_liquidity(&mut router, "user", assets, None)
        .unwrap_err();
    assert_eq!(
        &err.to_string(),
        "Generic error: Provided token does not belong to the pair"
    );

    let assets = helper.assets.with_balances(0, 1000);
    helper.give_coins(&mut router, "user", &assets[1]);
    let err = helper
        .provide_liquidity(&mut router, "user", assets, None)
        .unwrap_err();
    assert_eq!(&err.to_string(), "Event of zero transfer");

    helper
        .withdraw_liquidity(&mut router, "user", 60u128)
        .unwrap();
    let lp_balance = helper
        .get_token_balance(&mut router, &helper.lp_token, "user")
        .unwrap();
    assert_eq!(lp_balance, 40u128);
    let lp_balance = helper
        .get_token_balance(&mut router, &helper.btc_token, "user")
        .unwrap();
    assert_eq!(lp_balance, 60u128);

    let err = helper
        .withdraw_liquidity(&mut router, "user2", 51u128)
        .unwrap_err();
    assert_eq!(&err.to_string(), "Overflow: Cannot Sub with 50 and 51");

    helper
        .withdraw_liquidity(&mut router, "user2", 50u128)
        .unwrap();
    let lp_balance = helper
        .get_token_balance(&mut router, &helper.lp_token, "user2")
        .unwrap();
    assert_eq!(lp_balance, 0u128);
    let lp_balance = helper
        .get_token_balance(&mut router, &helper.btc_token, "user2")
        .unwrap();
    assert_eq!(lp_balance, 50u128);
}

#[test]
fn check_update_owner() {
    let mut app = mock_app();
    let owner = Addr::unchecked("owner");
    let helper = Helper::init(&mut app, &owner);

    let new_owner = String::from("new_owner");

    // New owner
    let msg = ExecuteMsg::ProposeNewOwner {
        new_owner: new_owner.clone(),
        expires_in: 100, // seconds
    };

    // Unauthed check
    let err = app
        .execute_contract(Addr::unchecked("not_owner"), helper.pair.clone(), &msg, &[])
        .unwrap_err();
    assert_eq!(err.to_string(), "Generic error: Unauthorized");

    // Claim before proposal
    let err = app
        .execute_contract(
            Addr::unchecked(new_owner.clone()),
            helper.pair.clone(),
            &ExecuteMsg::ClaimOwnership {},
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.to_string(),
        "Generic error: Ownership proposal not found"
    );

    // Propose new owner
    app.execute_contract(Addr::unchecked("owner"), helper.pair.clone(), &msg, &[])
        .unwrap();

    // Claim from invalid addr
    let err = app
        .execute_contract(
            Addr::unchecked("invalid_addr"),
            helper.pair.clone(),
            &ExecuteMsg::ClaimOwnership {},
            &[],
        )
        .unwrap_err();
    assert_eq!(err.to_string(), "Generic error: Unauthorized");

    // Claim ownership
    app.execute_contract(
        Addr::unchecked(new_owner.clone()),
        helper.pair.clone(),
        &ExecuteMsg::ClaimOwnership {},
        &[],
    )
    .unwrap();

    // Let's query the contract state
    let msg = QueryMsg::Config {};
    let res: ConfigResponse = app.wrap().query_wasm_smart(&helper.pair, &msg).unwrap();

    assert_eq!(res.owner, new_owner)
}

#[test]
fn check_swap() {
    let mut app = mock_app();
    let owner = Addr::unchecked("owner");
    let helper = Helper::init(&mut app, &owner);

    // Filling up the LP pool
    helper
        .update_whitelist(&mut app, "owner", vec!["owner"], vec![])
        .unwrap();
    let lp_assets = helper.assets.with_balances(1000_000000, 0);
    helper
        .provide_liquidity(&mut app, "owner", lp_assets, None)
        .unwrap();

    let assets = helper.assets.with_balances(1, 20000_000000);
    helper.give_coins(&mut app, "user", &assets[0]);

    let err = helper
        .native_swap(&mut app, "user", &assets[0], false)
        .unwrap_err();
    assert_eq!(&err.to_string(), "Unauthorized");

    // There is no ust in the pool as there were no swaps yet
    let err = helper.cw20_swap(&mut app, "user", &assets[0]).unwrap_err();
    assert_eq!(&err.to_string(), "Ask pool is empty");

    let err = helper
        .native_swap(&mut app, "user", &assets[1], false)
        .unwrap_err();
    assert_eq!(
        &err.to_string(),
        "Generic error: Native token balance mismatch between the argument and the transferred"
    );

    helper.give_coins(&mut app, "user", &assets[1]);
    let ust_balance = app.wrap().query_balance("user", "uusd").unwrap();
    // 20k ust + 1.39 ust tax fee
    assert_eq!(ust_balance.amount.u128(), 20001_390000);
    helper
        .native_swap(&mut app, "user", &assets[1], true)
        .unwrap();
    let btc_balance = helper
        .get_token_balance(&mut app, &helper.btc_token, "user")
        .unwrap();
    // 0.5 BTC - spread fee
    assert_eq!(btc_balance, 499751u128);
    let ust_balance = app.wrap().query_balance("user", "uusd").unwrap();
    assert_eq!(ust_balance.amount.u128(), 0);

    // 1MM$
    let assets = helper.assets.with_balances(1, 5_000000_000000);
    helper.give_coins(&mut app, "rich_person", &assets[1]);
    helper
        .native_swap(&mut app, "rich_person", &assets[1], true)
        .unwrap();
    let btc_balance = helper
        .get_token_balance(&mut app, &helper.btc_token, "rich_person")
        .unwrap();
    // Spread fee 0.24%
    assert_eq!(btc_balance, 124_700117);
    let ust_balance = app.wrap().query_balance("rich_person", "uusd").unwrap();
    assert_eq!(ust_balance.amount.u128(), 0);

    let ust_balance = app.wrap().query_balance("trader", "uusd").unwrap();
    assert_eq!(ust_balance.amount.u128(), 0);
    let btc_asset = helper.assets[0].with_balance(1_000000);
    helper.give_coins(&mut app, "trader", &btc_asset);
    let btc_balance = helper
        .get_token_balance(&mut app, &helper.btc_token, "trader")
        .unwrap();
    assert_eq!(btc_balance, 1_000000);
    helper.cw20_swap(&mut app, "trader", &btc_asset).unwrap();
    let ust_balance = app.wrap().query_balance("trader", "uusd").unwrap();
    // 40k$ - 0.1% spread fee - 1.39 ust tax fee
    assert_eq!(ust_balance.amount.u128(), 39598_610000);
    let btc_balance = helper
        .get_token_balance(&mut app, &helper.btc_token, "trader")
        .unwrap();
    assert_eq!(btc_balance, 0);
}