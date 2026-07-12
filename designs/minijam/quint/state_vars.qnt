// Shared state variables. Importing this module gives all modules the same
// variables, so invariants can be nullary values.
module state_vars {
  import types.* from "./types"
  import messages.* from "./messages"
  import state.* from "./state"

  var mj: MiniJamState
  var now: BlockNumber

  // Ghost variables for transition invariants and trace-oriented tests.
  var prevMj: MiniJamState
  var lastExecution: ExecutionOutput
  var lastDelta: List[StateChange]
  var lastVotes: List[WorkerVote]
  var lastFault: BulletinFault
}
