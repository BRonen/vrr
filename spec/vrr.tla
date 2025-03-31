---- MODULE vrr ----
EXTENDS Integers, FiniteSets, Sequences

CONSTANTS Replicas,        \* Set of replica IDs
          F,               \* Maximum number of failures tolerated
          Operations      \* Set of possible client operations

ASSUME /\ F >= 0
       /\ F < Cardinality(Replicas) \div 2  \* 2F + 1 replicas required for majority
       /\ Replicas /= {}

VARIABLES 
    \* Per-replica state
    status,         \* status[r] is one of "normal", "view-change", "recovering"
    currentView,    \* currentView[r] is the current view number at replica r
    log,            \* log[r] is the replica's log of operations
    commitIndex,    \* commitIndex[r] is the index of highest committed operation
    
    \* Protocol messages
    messages        \* Set of all messages in the network

TypeInvariant ==
    /\ status \in [Replicas -> {"normal", "view-change", "recovering"}]
    /\ currentView \in [Replicas -> Nat]
    /\ log \in [Replicas -> Seq(Operations)]
    /\ commitIndex \in [Replicas -> Nat]
    /\ messages \subseteq [
         type: {"PrepareMsg", "PrepareOkMsg", "CommitMsg", 
                "StartViewChange", "DoViewChange", "StartView"},
         src: Replicas,
         dst: Replicas,
         view: Nat
       ]

Init ==
    /\ status = [r \in Replicas |-> "normal"]
    /\ currentView = [r \in Replicas |-> 0]
    /\ log = [r \in Replicas |-> << >>]
    /\ commitIndex = [r \in Replicas |-> 0]
    /\ messages = {}

Next == TRUE

Spec == Init /\ []Next

----

THEOREM Spec => []TypeInvariant

====