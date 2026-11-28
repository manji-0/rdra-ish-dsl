/-!
# A certificate of the partition of a run of `X` and its Boolean check

The data the Rust test `dump_partition_certificates` writes for one flat run of the extended
sub-lifecycle `X` that `Classes::of_graph` partitions, by state number `0 … n-1` and class
number `0 … nb-1`:

- per state, its `obs` value (`ov`), its `S` value (`sv`) and its input bytes (`iv`), each
  value numbered as it first shows, and its class (`blk`);
- per class, its representative state (`rep`, `Classes::reps`);
- the run's initial states (`init`), its silent edges (`ssrc`, `sdst`) and its rises (`rsrc`,
  `rdst`), as `split_edges` gives them.

`check` computes a `Witness` (untrusted code: anything it computes is tested) and tests it
with `validate`, a conjunction of loops over the numbers that read each array at a few
places per step. `PartitionCheck.lean` proves that `validate` of any witness gives
`Sweep.Cert` of the run, so `Stable` (`Reduction.lean`) on a complete run, and the shape of
`X` (`Extended`) on its reachable part. The executable `partition-check`
(`PartitionCheckMain.lean`) runs `check` on each record. This file imports nothing, so the
executable compiles it alone.

The parts:

- **well-formed**: the arrays have their lengths and every number is in range: each class
  below `nb`, and each representative, initial state and edge end below `n` (the soundness
  proof reads the arrays only through `g` and uses these bounds);
- **rep**: each class's representative lies in it;
- **b1**: each state has its representative's `obs` and input bytes;
- **b2**: the exits of each class (the classes a silent edge from it enters, `eoff`,
  `elist`) hold every silent edge between two classes (`eix`), and each state of a class
  reaches each exit of its class by silent edges through the class, read off a ranked
  successor per state and exit (`poff`, `pvia`, `prank`);
- **b3**: the classes each state enters by a rise (`roff`, `rlist`), witnessed both ways
  (`rix`, `rwit`), are its representative's;
- **silent keeps inputs**, **rise keeps S**: the shape of `X` (Definition 4);
- **reach**: every state is reached from an initial state, read off a ranked parent per
  state (`pk`, `pe`, `prk`).
-/

namespace RdraIsh.PCert

/-- A record: one numbered run of `X` and its partition. -/
structure Data where
  n : Nat
  nb : Nat
  ov : Array Nat
  sv : Array Nat
  iv : Array Nat
  blk : Array Nat
  rep : Array Nat
  init : Array Nat
  ssrc : Array Nat
  sdst : Array Nat
  rsrc : Array Nat
  rdst : Array Nat

/-- What `check` computes and `validate` tests. -/
structure Witness where
  eoff : Array Nat
  elist : Array Nat
  eix : Array Nat
  poff : Array Nat
  pvia : Array Nat
  prank : Array Nat
  roff : Array Nat
  rlist : Array Nat
  rix : Array Nat
  rwit : Array Nat
  pk : Array Nat
  pe : Array Nat
  prk : Array Nat

/-- An array read as a function, `0` past its end. -/
@[inline] def g (a : Array Nat) (i : Nat) : Nat := a.getD i 0

namespace Data

variable (d : Data)

/-- The number of silent edges and of rises. -/
def ne : Nat := d.ssrc.size
def nr : Nat := d.rsrc.size

def blkOf (i : Nat) : Nat := g d.blk i
def repOf (c : Nat) : Nat := g d.rep c

/-- The silent edges and the rises as pairs of states. -/
def silentList : List (Nat × Nat) := (List.range d.ne).map fun e => (g d.ssrc e, g d.sdst e)
def riseList : List (Nat × Nat) := (List.range d.nr).map fun e => (g d.rsrc e, g d.rdst e)

def wellFormed : Bool :=
  d.ov.size == d.n && d.sv.size == d.n && d.iv.size == d.n && d.blk.size == d.n &&
    d.rep.size == d.nb && d.sdst.size == d.ne && d.rdst.size == d.nr &&
    Nat.all d.n (fun i _ => decide (d.blkOf i < d.nb)) &&
    Nat.all d.nb (fun c _ => decide (d.repOf c < d.n)) &&
    Nat.all d.init.size (fun k _ => decide (g d.init k < d.n)) &&
    Nat.all d.ne (fun e _ => decide (g d.ssrc e < d.n) && decide (g d.sdst e < d.n)) &&
    Nat.all d.nr (fun e _ => decide (g d.rsrc e < d.n) && decide (g d.rdst e < d.n))

/-- Each class's representative lies in it. -/
def checkRep : Bool :=
  Nat.all d.nb fun c _ => decide (d.repOf c < d.n) && d.blkOf (d.repOf c) == c

/-- (B1): each state has its representative's `obs` value and input bytes. -/
def checkB1 : Bool :=
  Nat.all d.n fun i _ =>
    g d.ov i == g d.ov (d.repOf (d.blkOf i)) && g d.iv i == g d.iv (d.repOf (d.blkOf i))

variable (w : Witness)

/-- Every silent edge between two classes enters an exit of its source's class. -/
def checkClosed : Bool :=
  Nat.all d.ne fun e _ =>
    let p := g d.ssrc e
    let t := g d.sdst e
    p == t || d.blkOf p == d.blkOf t ||
      (decide (g w.eoff (d.blkOf p) ≤ g w.eix e) &&
        decide (g w.eix e < g w.eoff (d.blkOf p + 1)) && g w.elist (g w.eix e) == d.blkOf t)

/-- The `j`-th exit of `p`'s class, and the number of exits `p` has a pair for. -/
def exitAt (p j : Nat) : Nat := g w.elist (g w.eoff (d.blkOf p) + j)
def segLen (p : Nat) : Nat := g w.poff (p + 1) - g w.poff p

/-- The pair of `p` and its class's `j`-th exit `sp`: an edge from `p` straight into `sp`
from outside it, or to a state of `p`'s class whose pair for `sp` has a lower rank. -/
def pairOk (p j : Nat) : Bool :=
  let k := g w.poff p + j
  let e := g w.pvia k
  let t := g d.sdst e
  let sp := d.exitAt w p j
  decide (e < d.ne) && g d.ssrc e == p && t != p &&
    ((d.blkOf t == sp && d.blkOf p != sp) ||
      (d.blkOf t == d.blkOf p && decide (t < d.n) && decide (j < segLen w t) &&
        decide (g w.prank (g w.poff t + j) < g w.prank k)))

/-- Each state has a pair for every exit of its class, and each pair holds. -/
def checkPairs : Bool :=
  Nat.all d.n fun p _ =>
    segLen w p == g w.eoff (d.blkOf p + 1) - g w.eoff (d.blkOf p) &&
      Nat.all (segLen w p) fun j _ => d.pairOk w p j

/-- (B2). -/
def checkB2 : Bool := d.checkClosed w && d.checkPairs w

/-- Every rise enters a class listed for its source. -/
def checkEntered : Bool :=
  Nat.all d.nr fun e _ =>
    decide (g w.roff (g d.rsrc e) ≤ g w.rix e) && decide (g w.rix e < g w.roff (g d.rsrc e + 1)) &&
      g w.rlist (g w.rix e) == d.blkOf (g d.rdst e)

/-- Every class listed for a state is entered by one of its rises. -/
def checkWit : Bool :=
  Nat.all d.n fun p _ =>
    Nat.all (g w.roff (p + 1) - g w.roff p) fun j _ =>
      decide (g w.rwit (g w.roff p + j) < d.nr) && g d.rsrc (g w.rwit (g w.roff p + j)) == p &&
        d.blkOf (g d.rdst (g w.rwit (g w.roff p + j))) == g w.rlist (g w.roff p + j)

/-- Each state's list is its representative's. -/
def checkSame : Bool :=
  Nat.all d.n fun p _ =>
    let q := d.repOf (d.blkOf p)
    g w.roff (p + 1) - g w.roff p == g w.roff (q + 1) - g w.roff q &&
      Nat.all (g w.roff (p + 1) - g w.roff p) fun j _ =>
        g w.rlist (g w.roff p + j) == g w.rlist (g w.roff q + j)

/-- (B3). -/
def checkB3 : Bool := d.checkEntered w && d.checkWit w && d.checkSame w

/-- Definition 4's shape: a silent edge keeps the input bytes, a rise keeps `S`. -/
def checkSilentInp : Bool := Nat.all d.ne fun e _ => g d.iv (g d.ssrc e) == g d.iv (g d.sdst e)
def checkRiseS : Bool := Nat.all d.nr fun e _ => g d.sv (g d.rsrc e) == g d.sv (g d.rdst e)

/-- Every state is initial, or entered by an edge from a state of lower rank. -/
def checkReach : Bool :=
  Nat.all d.n fun i _ =>
    (g w.pk i == 0 && decide (g w.pe i < d.init.size) && g d.init (g w.pe i) == i) ||
    (g w.pk i == 1 && decide (g w.pe i < d.ne) && g d.sdst (g w.pe i) == i &&
      decide (g d.ssrc (g w.pe i) < d.n) && decide (g w.prk (g d.ssrc (g w.pe i)) < g w.prk i)) ||
    (g w.pk i == 2 && decide (g w.pe i < d.nr) && g d.rdst (g w.pe i) == i &&
      decide (g d.rsrc (g w.pe i) < d.n) && decide (g w.prk (g d.rsrc (g w.pe i)) < g w.prk i))

/-- Every part but the well-formedness, on a witness. -/
def validate : Bool :=
  d.checkRep && d.checkB1 && d.checkB2 w && d.checkB3 w && d.checkSilentInp && d.checkRiseS &&
    d.checkReach w

/-! ## The witness (untrusted: `validate` tests what it computes) -/

/-- The values `vals` grouped by `keys` (each below `m`): offsets of size `m + 1`, then the
values in order of key. -/
def csr (m : Nat) (keys vals : Array Nat) : Array Nat × Array Nat := Id.run do
  let mut off := Array.replicate (m + 1) 0
  for k in keys do
    off := off.modify (k + 1) (· + 1)
  for i in [0:m] do
    off := off.set! (i + 1) (off[i]! + off[i + 1]!)
  let mut fill := off
  let mut out := Array.replicate keys.size 0
  for i in [0:keys.size] do
    let k := keys[i]!
    out := out.set! fill[k]! vals[i]!
    fill := fill.modify k (· + 1)
  return (off, out)

/-- The index in `[lo, hi)` of `x` in the ascending `a`, or `hi`. -/
def search (a : Array Nat) (lo hi x : Nat) : Nat := Id.run do
  let mut l := lo
  let mut h := hi
  for _ in [0:64] do
    if l < h then
      let m := (l + h) / 2
      if a[m]! < x then l := m + 1 else h := m
  return if l < hi && a[l]! == x then l else hi

/-- Prefix sums of `len`, of size `len.size + 1`. -/
def offsets (len : Array Nat) : Array Nat := Id.run do
  let mut off := Array.replicate (len.size + 1) 0
  for i in [0:len.size] do
    off := off.set! (i + 1) (off[i]! + len[i]!)
  return off

/-- The witness: the exits of each class and the pairs by a search back from each class
over the silent edges into it, the classes each state's rises enter, and a search forward
from the initial states. -/
def witness : Witness := Id.run do
  let n := d.n
  let nb := d.nb
  let ne := d.ne
  let nr := d.nr
  -- The exits of each class, ascending.
  let mut keys : Array Nat := #[]
  for e in [0:ne] do
    let p := g d.ssrc e
    let t := g d.sdst e
    if p != t && d.blkOf p != d.blkOf t then
      keys := keys.push (d.blkOf p * nb + d.blkOf t)
  keys := keys.qsort (· < ·)
  let mut elen := Array.replicate nb 0
  let mut elist : Array Nat := #[]
  let mut last := nb * nb
  for k in keys do
    if k != last then
      elist := elist.push (k % nb)
      elen := elen.modify (k / nb) (· + 1)
      last := k
  let eoff := offsets elen
  let exitIx := fun (b sp : Nat) => search elist (g eoff b) (g eoff (b + 1)) sp
  let mut eix := Array.replicate ne 0
  for e in [0:ne] do
    let p := g d.ssrc e
    let t := g d.sdst e
    if p != t && d.blkOf p != d.blkOf t then
      eix := eix.set! e (exitIx (d.blkOf p) (d.blkOf t))
  -- The pairs, by a search back from each exit.
  let poff := offsets ((Array.range n).map fun p => g elen (d.blkOf p))
  let total := g poff n
  let mut pvia := Array.replicate total ne
  let mut prank := Array.replicate total 0
  let (predOff, predE) := csr n d.sdst (Array.range ne)
  let (memOff, mem) := csr nb d.blk (Array.range n)
  for sp in [0:nb] do
    let mut queue : Array (Nat × Nat) := #[]
    for m in [g memOff sp:g memOff (sp + 1)] do
      let t := g mem m
      for q in [g predOff t:g predOff (t + 1)] do
        let e := g predE q
        let p := g d.ssrc e
        if p != t && d.blkOf p != sp then
          let j := exitIx (d.blkOf p) sp - g eoff (d.blkOf p)
          let k := g poff p + j
          if g pvia k == ne then
            pvia := pvia.set! k e
            prank := prank.set! k 0
            queue := queue.push (p, j)
    let mut head := 0
    while head < queue.size do
      let (x, j) := queue[head]!
      head := head + 1
      let r := g prank (g poff x + j)
      for q in [g predOff x:g predOff (x + 1)] do
        let e := g predE q
        let p := g d.ssrc e
        if p != x && d.blkOf p == d.blkOf x then
          let k := g poff p + j
          if g pvia k == ne then
            pvia := pvia.set! k e
            prank := prank.set! k (r + 1)
            queue := queue.push (p, j)
  -- The classes each state's rises enter, ascending, each with one rise.
  let (riseOff, riseE) := csr n d.rsrc (Array.range nr)
  let mut rlen := Array.replicate n 0
  let mut rlist : Array Nat := #[]
  let mut rwit : Array Nat := #[]
  for p in [0:n] do
    let mut out : Array (Nat × Nat) := #[]
    for q in [g riseOff p:g riseOff (p + 1)] do
      let e := g riseE q
      out := out.push (d.blkOf (g d.rdst e), e)
    out := out.qsort (fun a b => a.1 < b.1)
    let mut prev := nb
    for (c, e) in out do
      if c != prev then
        rlist := rlist.push c
        rwit := rwit.push e
        rlen := rlen.modify p (· + 1)
        prev := c
  let roff := offsets rlen
  let mut rix := Array.replicate nr 0
  for e in [0:nr] do
    let p := g d.rsrc e
    rix := rix.set! e (search rlist (g roff p) (g roff (p + 1)) (d.blkOf (g d.rdst e)))
  -- A search forward from the initial states: kind 0 initial, 1 silent, 2 rise, 3 unseen.
  let (succOff, succE) := csr n d.ssrc (Array.range ne)
  let mut pk := Array.replicate n 3
  let mut pe := Array.replicate n 0
  let mut prk := Array.replicate n 0
  let mut queue : Array Nat := #[]
  for k in [0:d.init.size] do
    let i := g d.init k
    if g pk i == 3 then
      pk := pk.set! i 0
      pe := pe.set! i k
      queue := queue.push i
  let mut head := 0
  while head < queue.size do
    let i := queue[head]!
    head := head + 1
    let r := g prk i
    for q in [g succOff i:g succOff (i + 1)] do
      let e := g succE q
      let t := g d.sdst e
      if g pk t == 3 then
        pk := pk.set! t 1
        pe := pe.set! t e
        prk := prk.set! t (r + 1)
        queue := queue.push t
    for q in [g riseOff i:g riseOff (i + 1)] do
      let e := g riseE q
      let t := g d.rdst e
      if g pk t == 3 then
        pk := pk.set! t 2
        pe := pe.set! t e
        prk := prk.set! t (r + 1)
        queue := queue.push t
  return { eoff, elist, eix, poff, pvia, prank, roff, rlist, rix, rwit, pk, pe, prk }

/-- The check of a record. -/
def check : Bool := d.wellFormed && d.validate d.witness

/-- The parts `check` fails, by name, for the executable's report. -/
def failures : List String :=
  if !d.wellFormed then ["well-formed"] else
    let w := d.witness
    [("rep", d.checkRep), ("b1", d.checkB1), ("b2", d.checkB2 w), ("b3", d.checkB3 w),
      ("silent keeps inputs", d.checkSilentInp), ("rise keeps S", d.checkRiseS),
      ("reach", d.checkReach w)].filterMap fun (n, ok) => if ok then none else some n

end Data

end RdraIsh.PCert
