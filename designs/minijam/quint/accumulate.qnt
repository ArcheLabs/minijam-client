// Abstract MiniJAM accumulate/executive.
module accumulate {
  import types.* from "./types"
  import messages.* from "./messages"
  import state.* from "./state"
  import service.* from "./service"
  import preimage.* from "./preimage"
  import exports_bookkeeping.* from "./export"
  import bridge.* from "./bridge"

  pure def applyStateChange(s: MiniJamState, serviceId: ServiceId, change: StateChange): MiniJamState =
    if (not(namespaceAllowed(change.key.namespace))) s
    else if (change.key.namespace == ServiceStorage or change.key.namespace == ServiceInfo or change.key.namespace == ServiceLookup)
      applyServiceChange(s, serviceId, change)
    else if (change.key.namespace == Preimage)
      applyPreimageChange(s, change)
    else match change.op {
      | Remove => { ...s, protocolState: mapRemove(s.protocolState, change.key) }
      | _ => match change.value {
        | Some(v) => { ...s, protocolState: s.protocolState.put(change.key, v) }
        | None => s
      }
    }

  pure def validDelta(changes: List[StateChange]): bool =
    changesAllowed(changes) and changesOrderedUnique(changes)

  pure def executionOutput(report: ReportEnvelope, adminChanges: List[StateChange]): ExecutionOutput = {
    orderedChanges: report.delta.concat(adminChanges),
    consumedReports: List(report.reportHash),
    serviceOutputs: report.outputs,
    bridgeEffects: report.bridgeEffects,
    gasUsed: report.gasUsed,
    receiptHash: hashOf(5000 + report.gasUsed),
    outcome: report.outcome
  }

  pure def executeReport(s: MiniJamState, report: ReportEnvelope): (MiniJamState, ExecutionOutput) = {
    val drained = consumeAdminRecords(s)
    val base = drained._1
    val out = executionOutput(report, drained._2)
    val fatal = report.outcome == FatalStateError or report.outcome == FatalInvariantError
    val yielded = report.outcome == OutOfGas or report.outcome == Trap or report.outcome == ServiceFailure
    val allChanges = out.orderedChanges
    val stale =
      report.parentStateRoot != serviceRoot(s, report.service) or
      report.postStateRoot != stateRootAfter(report.parentStateRoot, report.reportHash)
    val replay = s.appliedReports.contains(report.reportHash)
    val badEnvelope =
      report.protocolVersion != ProtocolVersion or
      report.chainId != ChainId or
      report.gasUsed > MaxGas
    if (stale or replay or badEnvelope or not(validDelta(allChanges))) (s, { ...out, outcome: FatalInvariantError })
    else if (fatal or yielded) (s, out)
    else {
      val afterDelta = allChanges.foldl(base, (acc, c) => applyStateChange(acc, report.service, c))
      val afterBridge = report.bridgeEffects.foldl(afterDelta, (acc, e) =>
        if (e.direction == BridgeOutbound) releaseOutbound(acc, e) else acc)
      val afterOutputs = recordReportExport(recordReportOutputs(afterBridge, report), report)
      val nextNonce = serviceNonceOf(afterOutputs, report.service) + 1
      ({
          ...afterOutputs,
          serviceRoots: afterOutputs.serviceRoots.put(report.service, report.postStateRoot),
          serviceNonce: afterOutputs.serviceNonce.put(report.service, nextNonce),
          appliedReports: afterOutputs.appliedReports.union(Set(report.reportHash)),
          acceptedReports: afterOutputs.acceptedReports.put(report.reportHash, report)
        }, out)
    }
  }
}
