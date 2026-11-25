// Driver of tests/codegen_differential.rs: one JSON case per stdin line, one JSON result per
// stdout line. The test appends the decoders, the encoders, and drvRun.
package main

import (
	"bufio"
	"bytes"
	"encoding/json"
	"fmt"
	"os"
)

func drvGet(j any, k string) any { return j.(map[string]any)[k] }

func drvInt(j any) int64 {
	n, err := j.(json.Number).Int64()
	if err != nil {
		panic(err)
	}
	return n
}

func drvStr(j any) string { return j.(string) }

func drvBool(j any) bool { return j.(bool) }

func drvList[T any](j any, f func(any) T) []T {
	var out []T
	for _, x := range j.([]any) {
		out = append(out, f(x))
	}
	return out
}

func drvOpt[T any](j any, f func(any) T) *T {
	if j == nil {
		return nil
	}
	v := f(j)
	return &v
}

func drvPtr[T any](p *T, f func(T) any) any {
	if p == nil {
		return nil
	}
	return f(*p)
}

func drvCase(line []byte) (out map[string]any) {
	defer func() {
		if p := recover(); p != nil {
			out = map[string]any{"panic": fmt.Sprint(p)}
		}
	}()
	dec := json.NewDecoder(bytes.NewReader(line))
	dec.UseNumber()
	var c map[string]any
	if err := dec.Decode(&c); err != nil {
		panic(err)
	}
	return drvRun(drvInt(c["f"]), c["a"].([]any))
}

func main() {
	in := bufio.NewScanner(os.Stdin)
	in.Buffer(make([]byte, 1<<20), 1<<28)
	out := bufio.NewWriter(os.Stdout)
	defer out.Flush()
	for in.Scan() {
		b, err := json.Marshal(drvCase(in.Bytes()))
		if err != nil {
			panic(err)
		}
		out.Write(b)
		out.WriteString("\n")
	}
}
