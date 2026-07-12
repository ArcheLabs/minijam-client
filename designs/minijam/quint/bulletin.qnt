// Bulletin-compatible simulator abstraction.
module bulletin {
  import types.* from "./types"
  import state.* from "./state"

  pure def authorize(s: MiniJamState, account: AccountId): MiniJamState =
    { ...s, bulletin: { ...s.bulletin, authorized: s.bulletin.authorized.union(Set(account)) } }

  pure def storeContent(s: MiniJamState, account: AccountId, contentHash: Hash, retention: BlockNumber): MiniJamState =
    if (not(s.bulletin.authorized.contains(account))) s
    else {
      ...s,
      bulletin: {
        ...s.bulletin,
        entries: s.bulletin.entries.put(contentHash, {
          contentHash: contentHash,
          retentionUntil: s.bulletin.blockNumber + retention,
          fault: NoBulletinFault
        })
      }
    }

  pure def injectFault(s: MiniJamState, contentHash: Hash, fault: BulletinFault): MiniJamState =
    if (not(s.bulletin.entries.keys().contains(contentHash))) s
    else {
      val e = s.bulletin.entries.get(contentHash)
      { ...s, bulletin: { ...s.bulletin, entries: s.bulletin.entries.put(contentHash, { ...e, fault: fault }) } }
    }

  pure def fetchStatus(s: MiniJamState, contentHash: Hash): BulletinFault =
    if (not(s.bulletin.entries.keys().contains(contentHash))) Missing
    else {
      val e = s.bulletin.entries.get(contentHash)
      if (e.fault != NoBulletinFault) e.fault
      else if (s.bulletin.blockNumber >= e.retentionUntil) Expired
      else NoBulletinFault
    }
}
