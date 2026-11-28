import RdraIsh.PartitionCert

/-! `lake exe partition-check FILE`: run `RdraIsh.PCert.Data.check` on every record of FILE,
as the Rust test `dump_partition_certificates` writes them (`partition/dump.rs`): a line
`c NAME`, then one line of numbers per field, each starting with its tag. The soundness of
`check` is `PartitionCheck.lean`'s. It exits 0 when every record is accepted and there is at
least one, and names each rejected one with the parts it fails. -/

open RdraIsh.PCert

/-- The numbers of a line after its tag. -/
def numbers (line : String) : Array Nat := Id.run do
  let mut out : Array Nat := #[]
  let mut cur := 0
  let mut inNum := false
  for c in line.toList.drop 1 do
    if c.isDigit then
      cur := cur * 10 + (c.toNat - '0'.toNat)
      inNum := true
    else if inNum then
      out := out.push cur
      cur := 0
      inNum := false
  if inNum then out := out.push cur
  return out

/-- Even entries and odd entries. -/
def unzipPairs (a : Array Nat) : Array Nat × Array Nat := Id.run do
  let mut x : Array Nat := #[]
  let mut y : Array Nat := #[]
  for i in [0:a.size / 2] do
    x := x.push a[2 * i]!
    y := y.push a[2 * i + 1]!
  return (x, y)

def parseRecord (fields : List String) : Except String Data := do
  let get (tag : String) : Except String (Array Nat) :=
    match fields.find? (·.startsWith (tag ++ " ") ) |>.orElse (fun _ => fields.find? (· == tag)) with
    | some l => pure (numbers l)
    | none => throw s!"no line {tag}"
  let nk ← get "n"
  let (ssrc, sdst) := unzipPairs (← get "e")
  let (rsrc, rdst) := unzipPairs (← get "u")
  return {
    n := nk.getD 0 0, nb := nk.getD 1 0, ov := ← get "o", sv := ← get "s", iv := ← get "i",
    blk := ← get "b", rep := ← get "r", init := ← get "z", ssrc, sdst, rsrc, rdst }

def main (args : List String) : IO UInt32 := do
  let [path] := args | IO.eprintln "usage: partition-check FILE"; return 2
  let text ← IO.FS.readFile path
  let mut accepted := 0
  let mut rejected := 0
  let mut states := 0
  -- Records start at a line `c NAME`.
  let mut records : Array (String × List String) := #[]
  for line in text.splitOn "\n" do
    if line.startsWith "c " then
      records := records.push ((line.drop 2).toString, [])
    else if !line.isEmpty then
      match records.back? with
      | some (name, ls) => records := records.pop.push (name, line :: ls)
      | none => IO.eprintln "partition-check: a line before the first record"; return 1
  for (name, fields) in records do
    match parseRecord fields with
    | .error e => IO.eprintln s!"partition-check: {name}: {e}"; rejected := rejected + 1
    | .ok d =>
      states := states + d.n
      if d.check then accepted := accepted + 1
      else
        IO.eprintln s!"partition-check: {name}: rejected: {d.failures}"
        rejected := rejected + 1
  IO.println s!"partition-check: {accepted} runs accepted ({states} states), {rejected} rejected"
  return if rejected == 0 && accepted > 0 then 0 else 1
