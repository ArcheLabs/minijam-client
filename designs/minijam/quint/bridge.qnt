// Native bridge ledger abstraction.
//
// The virtual JAM Executive no longer returns bridge effects or injected admin
// records. Bridge handling is modelled as a Runtime-only subsystem that may
// observe configured standard JAM state keys after a successful delta commit.
module bridge {
  import types.* from "./types"
  import state.* from "./state"

  pure def bridgeStateKey(effect: BridgeEffect): Hash =
    hashOf(if (effect.direction == BridgeInbound) effect.nonce else 1000 + effect.nonce)

  pure def bridgeIn(s: MiniJamState, account: AccountId, target: ServiceId, amount: Balance): MiniJamState =
    if (amount <= 0) s
    else {
      val nonce = s.bridge.nextInboundNonce
      val effect = {
        direction: BridgeInbound,
        nonce: nonce,
        service: target,
        account: account,
        amount: amount
      }
      val oldEscrow = if (s.bridge.escrow.keys().contains(account)) s.bridge.escrow.get(account) else 0
      {
        ...s,
        bridge: {
          ...s.bridge,
          nextInboundNonce: nonce + 1,
          escrow: s.bridge.escrow.put(account, oldEscrow + amount),
          observedStateKeys: s.bridge.observedStateKeys.union(Set(bridgeStateKey(effect)))
        }
      }
    }

  pure def releaseOutbound(s: MiniJamState, effect: BridgeEffect): MiniJamState =
    if (effect.direction != BridgeOutbound or effect.amount <= 0 or s.bridge.consumedOutbound.contains(effect.nonce)) s
    else {
      val oldEscrow = if (s.bridge.escrow.keys().contains(effect.account)) s.bridge.escrow.get(effect.account) else 0
      if (oldEscrow < effect.amount) s
      else {
        ...s,
        bridge: {
          ...s.bridge,
          escrow: s.bridge.escrow.put(effect.account, oldEscrow - effect.amount),
          consumedOutbound: s.bridge.consumedOutbound.union(Set(effect.nonce)),
          observedStateKeys: s.bridge.observedStateKeys.union(Set(bridgeStateKey(effect)))
        }
      }
    }
}
