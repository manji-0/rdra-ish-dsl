/-!
# A certificate of Lemma 5 and its Boolean check

The sets the code's run-time check of Lemma 5 reads (`split_faults`), for one cone system,
over the ids `0 … nv-1` of its variables and `0 … na-1` of its actions:

- per action, its reads `rd` (`action_reads` with `copy_reads`) and its writes `wr`
  (`action_writes`);
- the variables `monotone_vars` marks;
- the split: `T_v`, `S_v`, `T_a`, `S_a`, `obs`, `inputs`, and per action the reads its fold
  covers (`Fold::reads`, empty for an action without a fold);
- the seeds' reads, and the kept actions of every seed.

The Rust test `dump_lemma5_certificates` writes one certificate per split, and the
executable `lemma5-check` (`Lemma5CheckMain.lean`) runs `check` on each.
`Lemma5Check.lean::check_sound` proves that a certificate `check` accepts satisfies
`Split.lean`'s `Lemma5` for every model whose reads and writes are the certificate's. This
file imports nothing, so the executable compiles it alone.
-/

namespace RdraIsh.Cert

/-- A certificate: lists of variable ids and action ids. -/
structure Data where
  nv : Nat
  na : Nat
  rd : List (List Nat)
  wr : List (List Nat)
  mono : List Nat
  t : List Nat
  s : List Nat
  ta : List Nat
  sa : List Nat
  obs : List Nat
  inputs : List Nat
  folded : List (List Nat)
  seeds : List (List Nat)
  keep : List Nat

namespace Data

variable (d : Data)

def rdOf (a : Nat) : List Nat := d.rd.getD a []
def wrOf (a : Nat) : List Nat := d.wr.getD a []
def foldedOf (a : Nat) : List Nat := d.folded.getD a []

/-- An action of `S_a`: outside `T_a`, and it writes something. -/
def inSa (a : Nat) : Bool := !d.ta.contains a && !(d.wrOf a).isEmpty

/-- Every id is in range, and there is one read, write and fold list per action. -/
def wellFormed : Bool :=
  let v := fun (l : List Nat) => l.all (· < d.nv)
  d.rd.length == d.na && d.wr.length == d.na && d.folded.length == d.na &&
    d.rd.all v && d.wr.all v && d.folded.all v && d.seeds.all v &&
    v d.mono && v d.t && v d.s && v d.obs && v d.inputs &&
    (d.ta ++ d.sa ++ d.keep).all (· < d.na)

/-- `S_v` is the complement of `T_v` and `S_a` the writers outside `T_a`. -/
def checkSides : Bool :=
  (List.range d.nv).all (fun v => d.s.contains v != d.t.contains v) &&
    (List.range d.na).all (fun a => d.sa.contains a == d.inSa a)

/-- (a) every `S_a` action writes only `S_v`. -/
def checkA : Bool :=
  (List.range d.na).all fun a => !d.inSa a || (d.wrOf a).all (fun v => !d.t.contains v)

/-- (b) every `T_a` action writes only `T_v`, and reads `S_v` only in `obs`. -/
def checkBWr : Bool := d.ta.all fun a => (d.wrOf a).all d.t.contains

def checkBRd : Bool :=
  d.ta.all fun a => (d.rdOf a).all fun v => d.t.contains v || d.obs.contains v

/-- `obs ⊆ S_v`. -/
def checkObsS : Bool := d.obs.all fun v => !d.t.contains v

/-- (c) every read of an `S_a` action is in `S_v`, an input, or folded. -/
def checkC : Bool :=
  (List.range d.na).all fun a => !d.inSa a ||
    (d.rdOf a).all fun v => !d.t.contains v || d.inputs.contains v || (d.foldedOf a).contains v

/-- (d) every input is monotone. -/
def checkD : Bool := d.inputs.all d.mono.contains

/-- (e) every seed reads `S_v` only in `obs`, and every kept action is in `T_a`. -/
def checkESeed : Bool :=
  d.seeds.all fun r => r.all fun v => d.t.contains v || d.obs.contains v

def checkEKeep : Bool := d.keep.all d.ta.contains

/-- `inputs ⊆ T_v`. -/
def checkInputsT : Bool := d.inputs.all d.t.contains

/-- The check of a certificate. -/
def check : Bool :=
  d.wellFormed && d.checkSides && d.checkA && d.checkBWr && d.checkBRd && d.checkObsS &&
    d.checkC && d.checkD && d.checkESeed && d.checkEKeep && d.checkInputsT

/-- The parts `check` fails, by name, for the executable's report. -/
def failures : List String :=
  [("well-formed", d.wellFormed), ("sides", d.checkSides), ("a", d.checkA),
    ("b writes", d.checkBWr), ("b reads", d.checkBRd), ("obs in S", d.checkObsS),
    ("c", d.checkC), ("d", d.checkD), ("e seeds", d.checkESeed), ("e kept", d.checkEKeep),
    ("inputs in T", d.checkInputsT)].filterMap fun (n, ok) => if ok then none else some n

end Data

end RdraIsh.Cert
