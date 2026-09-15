// MiniJAM single-Worker direct-Refine model.
//
// Formal RPC owns transaction admission and batching. The runtime only sees
// canonical reports from the configured Worker and records package outcomes.
module direct_ingress {
  type Hash = int
  type PackageStatus = Pending | Imported | Failed
  type TransactionStatus = Queued | Packaged | Refining | Reported | ImportedTx | FailedTx
  type Option[a] = Some(a) | None

  type Transaction = {
    id: Hash,
    serviceId: int,
    serviceCodeHash: Hash,
    payload: Hash
  }

  type Package = {
    packageHash: Hash,
    serviceId: int,
    serviceCodeHash: Hash,
    context: Hash,
    transactionIds: Set[Hash]
  }

  type MiniJamState = {
    worker: Hash,
    transactions: Hash -> Transaction,
    transactionStatuses: Hash -> TransactionStatus,
    packageStatuses: Hash -> PackageStatus,
    packages: Hash -> Package,
    activePackage: Option[Package],
    reports: Set[Hash],
    receipts: Hash -> Hash
  }

  pure def hashOf(value: int): Hash = value

  pure def transactionId(tx: Transaction): Hash =
    hashOf(tx.serviceId + tx.serviceCodeHash + tx.payload)

  pure val emptyMiniJam: MiniJamState = {
    worker: hashOf(1),
    transactions: Map(),
    transactionStatuses: Map(),
    packageStatuses: Map(),
    packages: Map(),
    activePackage: None,
    reports: Set(),
    receipts: Map()
  }

  pure def submitTransaction(s: MiniJamState, tx: Transaction): MiniJamState =
    if (s.transactions.keys().contains(tx.id)) s
    else {
      ...s,
      transactions: s.transactions.put(tx.id, tx),
      transactionStatuses: s.transactionStatuses.put(tx.id, Queued)
    }

  pure def markTransactions(
    s: MiniJamState,
    ids: Set[Hash],
    status: TransactionStatus
  ): MiniJamState =
    ids.fold(s, (acc, id) =>
      if (acc.transactions.keys().contains(id))
        { ...acc, transactionStatuses: acc.transactionStatuses.put(id, status) }
      else acc
    )

  pure def packageBatch(
    s: MiniJamState,
    packageHash: Hash,
    serviceId: int,
    serviceCodeHash: Hash,
    context: Hash,
    ids: Set[Hash]
  ): MiniJamState =
    if (ids == Set() or s.activePackage != None or s.packageStatuses.keys().contains(packageHash)) s
    else {
      val pkg: Package = {
        packageHash: packageHash,
        serviceId: serviceId,
        serviceCodeHash: serviceCodeHash,
        context: context,
        transactionIds: ids
      }
      val packaged = markTransactions(s, ids, Packaged)
      {
        ...packaged,
        packageStatuses: packaged.packageStatuses.put(packageHash, Pending),
        packages: packaged.packages.put(packageHash, pkg),
        activePackage: Some(pkg)
      }
    }

  pure def beginRefine(s: MiniJamState, packageHash: Hash): MiniJamState =
    if (s.packageStatuses.get(packageHash) != Pending) s
    else markTransactions(s, s.packages.get(packageHash).transactionIds, Refining)

  pure def submitReport(s: MiniJamState, origin: Hash, packageHash: Hash): MiniJamState =
    if (origin != s.worker or s.packageStatuses.get(packageHash) != Pending) s
    else {
      val reported = markTransactions(s, s.packages.get(packageHash).transactionIds, Reported)
      {
        ...reported,
        reports: s.reports.union(Set(packageHash))
      }
    }

  pure def importPackage(s: MiniJamState, packageHash: Hash, receipt: Hash): MiniJamState =
    if (not(s.reports.contains(packageHash)) or s.packageStatuses.get(packageHash) != Pending) s
    else {
      val imported = markTransactions(s, s.packages.get(packageHash).transactionIds, ImportedTx)
      {
        ...imported,
        packageStatuses: imported.packageStatuses.put(packageHash, Imported),
        receipts: imported.receipts.put(packageHash, receipt),
        activePackage: None
      }
    }

  pure def failPackage(s: MiniJamState, packageHash: Hash): MiniJamState =
    if (not(s.packageStatuses.keys().contains(packageHash))) s
    else {
      val failed = markTransactions(s, s.packages.get(packageHash).transactionIds, FailedTx)
      {
        ...failed,
        packageStatuses: failed.packageStatuses.put(packageHash, Failed),
        activePackage: None
      }
    }

  run directBatchAndReportTest = {
    val tx1: Transaction = {
      id: hashOf(11), serviceId: 7, serviceCodeHash: hashOf(70), payload: hashOf(1)
    }
    val tx2: Transaction = {
      id: hashOf(12), serviceId: 7, serviceCodeHash: hashOf(70), payload: hashOf(2)
    }
    val queued = submitTransaction(submitTransaction(emptyMiniJam, tx1), tx2)
    val packaged = packageBatch(queued, hashOf(99), 7, hashOf(70), hashOf(123), Set(tx1.id, tx2.id))
    val reported = submitReport(beginRefine(packaged, hashOf(99)), hashOf(1), hashOf(99))
    val imported = importPackage(reported, hashOf(99), hashOf(555))
    all {
      assert(imported.packageStatuses.get(hashOf(99)) == Imported),
      assert(imported.transactionStatuses.get(tx1.id) == ImportedTx),
      assert(imported.transactionStatuses.get(tx2.id) == ImportedTx),
      assert(imported.receipts.get(hashOf(99)) == hashOf(555)),
      assert(imported.activePackage == None)
    }
  }

  run wrongWorkerCannotReportTest = {
    val tx: Transaction = {
      id: hashOf(11), serviceId: 7, serviceCodeHash: hashOf(70), payload: hashOf(1)
    }
    val queued = submitTransaction(emptyMiniJam, tx)
    val packaged = packageBatch(queued, hashOf(99), 7, hashOf(70), hashOf(123), Set(tx.id))
    val rejected = submitReport(packaged, hashOf(2), hashOf(99))
    all {
      assert(rejected == packaged),
      assert(rejected.packageStatuses.get(hashOf(99)) == Pending)
    }
  }
}
