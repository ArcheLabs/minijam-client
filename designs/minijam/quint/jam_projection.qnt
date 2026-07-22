// Abstract projection of retained standard JAM state.
module jam_projection {
  import types.* from "./types"
  import messages.* from "./messages"

  type JamProjection = {
    slot: int,
    epoch: int,
    recentParentRoot: Hash,
    recentHeaderHash: Hash,
    safroleTransitions: int,
    readyReports: Set[Hash],
    accumulatedReports: Set[Hash],
    authorizationConsumed: Set[Hash],
    preparedPreimages: Set[Hash],
    appliedPreimages: Set[Hash],
    currentHistoryPackages: Set[Hash],
    accumulateInputReports: Set[Hash],
    preimageRequestsAtPrepare: Set[Hash],
    opaqueStateCommitment: Hash
  }

  pure val emptyJamProjection: JamProjection = {
    slot: 0,
    epoch: 0,
    recentParentRoot: hashOf(0),
    recentHeaderHash: hashOf(0),
    safroleTransitions: 0,
    readyReports: Set(),
    accumulatedReports: Set(),
    authorizationConsumed: Set(),
    preparedPreimages: Set(),
    appliedPreimages: Set(),
    currentHistoryPackages: Set(),
    accumulateInputReports: Set(),
    preimageRequestsAtPrepare: Set(),
    opaqueStateCommitment: hashOf(0)
  }

  pure def dependenciesMet(jam: JamProjection, report: CanonicalWorkReport): bool =
    report.dependencies.forall(dep => jam.accumulatedReports.contains(dep))

  pure def progressLastHistory(jam: JamProjection, parentStateRoot: Hash): JamProjection =
    { ...jam, recentParentRoot: parentStateRoot }

  pure def progressSafrole(jam: JamProjection, slot: int, entropy: Hash, tickets: Set[Hash]): JamProjection =
    if (tickets != Set() or slot <= jam.slot) jam
    else {
      ...jam,
      slot: slot,
      epoch: slot / EpochLength,
      safroleTransitions: jam.safroleTransitions + 1,
      opaqueStateCommitment: hashOf(jam.opaqueStateCommitment.hashBytes + entropy.hashBytes + slot)
    }

  pure def preparePreimages(jam: JamProjection, preimages: List[CanonicalPreimage]): JamProjection = {
    val hashes = preimages.indices().map(i => preimages[i].preimageHash)
    {
      ...jam,
      preparedPreimages: hashes,
      preimageRequestsAtPrepare: jam.preimageRequestsAtPrepare.union(hashes)
    }
  }

  pure def progressAccumulate(jam: JamProjection, reports: List[CanonicalWorkReport]): JamProjection = {
    val reportSet = reports.indices().map(i => reports[i].reportHash)
    val newlyAccumulated = reports.indices().filter(i => dependenciesMet(jam, reports[i])).map(i => reports[i].reportHash)
    {
      ...jam,
      readyReports: jam.readyReports.union(reportSet).exclude(newlyAccumulated),
      accumulatedReports: jam.accumulatedReports.union(newlyAccumulated),
      accumulateInputReports: reportSet
    }
  }

  pure def progressCurrentHistory(
    jam: JamProjection,
    headerHash: Hash,
    accumulateRoot: Hash,
    reports: List[CanonicalWorkReport]
  ): JamProjection =
    {
      ...jam,
      recentHeaderHash: headerHash,
      currentHistoryPackages: reports.indices().map(i => reports[i].packageHash),
      opaqueStateCommitment: hashOf(jam.opaqueStateCommitment.hashBytes + accumulateRoot.hashBytes)
    }

  pure def progressAuthorizations(jam: JamProjection, reports: List[CanonicalWorkReport]): JamProjection =
    { ...jam, authorizationConsumed: reports.indices().map(i => reports[i].authorizerHash) }

  pure def progressStatistics(jam: JamProjection, input: ExecutionInput): JamProjection =
    {
      ...jam,
      opaqueStateCommitment: hashOf(jam.opaqueStateCommitment.hashBytes + input.preimages.length() + input.reports.length())
    }

  pure def applyPreimages(jam: JamProjection, preimages: List[CanonicalPreimage]): JamProjection = {
    val applicable = preimages.indices()
      .filter(i => jam.preimageRequestsAtPrepare.contains(preimages[i].preimageHash))
      .map(i => preimages[i].preimageHash)
    {
      ...jam,
      appliedPreimages: jam.appliedPreimages.union(applicable),
      preparedPreimages: Set()
    }
  }
}
