// Service output/export bookkeeping.
module exports_bookkeeping {
  import types.* from "./types"
  import messages.* from "./messages"
  import state.* from "./state"

  pure def recordReportOutputs(s: MiniJamState, report: ReportEnvelope): MiniJamState =
    report.outputs.foldl(s, (acc, out) =>
      { ...acc, serviceOutputs: acc.serviceOutputs.put((report.workId, out.outputHash), out) })

  pure def recordReportExport(s: MiniJamState, report: ReportEnvelope): MiniJamState =
    { ...s, exports: s.exports.put((report.workId, report.metadata.exportsRoot), report.metadata.exportsRoot) }
}
