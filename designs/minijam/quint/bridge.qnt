// Native bridge and admin-service record abstraction.
module bridge {
  import types.* from "./types"
  import messages.* from "./messages"
  import state.* from "./state"

  pure def adminBridgeKey(effect: BridgeEffect): ProtocolKey = {
    namespace: AdminBridge,
    local: if (effect.direction == BridgeInbound) effect.nonce else 1000 + effect.nonce
  }

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
          adminRecords: s.bridge.adminRecords.put(adminBridgeKey(effect), amount)
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
          adminRecords: s.bridge.adminRecords.put(adminBridgeKey(effect), effect.amount)
        }
      }
    }

  pure def consumeAdminRecords(s: MiniJamState): (MiniJamState, List[StateChange]) = {
    val records = s.bridge.adminRecords.keys().fold(List(), (acc, k) => acc.append({
      key: k,
      op: Upsert,
      value: Some(s.bridge.adminRecords.get(k))
    }))
    ({ ...s, bridge: { ...s.bridge, adminRecords: Map() } }, records)
  }
}
