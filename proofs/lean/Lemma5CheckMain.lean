import Lean.Data.Json
import RdraIsh.Lemma5Cert

/-! `lake exe lemma5-check FILE`: run `RdraIsh.Cert.Data.check` on every certificate of
FILE, one JSON object per line, as the Rust test `dump_lemma5_certificates` writes them.
`Lemma5Check.lean::check_sound` is what an accepted certificate means. It exits 0 when every
certificate is accepted and there is at least one, and names each rejected one with the
parts it fails. -/

open Lean RdraIsh.Cert

def parseData (j : Json) : Except String Data := do
  let nat (k : String) : Except String Nat := j.getObjValAs? Nat k
  let ids (k : String) : Except String (List Nat) := j.getObjValAs? (List Nat) k
  let sets (k : String) : Except String (List (List Nat)) := j.getObjValAs? (List (List Nat)) k
  return {
    nv := ← nat "nv", na := ← nat "na", rd := ← sets "rd", wr := ← sets "wr",
    mono := ← ids "mono", t := ← ids "t", s := ← ids "s", ta := ← ids "ta",
    sa := ← ids "sa", obs := ← ids "obs", inputs := ← ids "inputs",
    folded := ← sets "folded", seeds := ← sets "seeds", keep := ← ids "keep" }

def main (args : List String) : IO UInt32 := do
  let [path] := args | IO.eprintln "usage: lemma5-check FILE"; return 2
  let text ← IO.FS.readFile path
  let mut accepted := 0
  let mut rejected := 0
  for line in text.splitOn "\n" do
    if line.isEmpty then continue
    match Json.parse line with
    | .error e => IO.eprintln s!"lemma5-check: unreadable line: {e}"; rejected := rejected + 1
    | .ok j =>
      let name := (j.getObjValAs? String "name").toOption.getD "?"
      match parseData j with
      | .error e => IO.eprintln s!"lemma5-check: {name}: {e}"; rejected := rejected + 1
      | .ok d =>
        if d.check then accepted := accepted + 1
        else
          IO.eprintln s!"lemma5-check: {name}: rejected: {d.failures}"
          rejected := rejected + 1
  IO.println s!"lemma5-check: {accepted} certificates accepted, {rejected} rejected"
  return if rejected == 0 && accepted > 0 then 0 else 1
