# Lowering to MIR (Middle-level Intermediate Representation)
## Goal
The goal of lowering to MIR is to 'desugar' the syntax into more simple control flow, that more closely resembles the way a computer
actually wants to execute instructions rather than in terms of a syntax tree of our language.
### Example 
An example of this would be "flattening" while loops (including break and continue statements) to comparisons and jumps. I.e:
```
let mut i = 0;
while i < 10 {
	if i == 1 {
		continue;
	}
	println(i);
	i = i + 1;
	if i == 8 {
		break;
	}
}
```
Would get converted to something akin to:
```
let mut i = 0;
wloop_1_cond: if i < 10 {
	if i == 1 {
		jump wloop_1_cond;
	}
	println(i);
	i = i + 1;
	if i == 8 {
		jump wloop_1_after;
	}
	jump wloop_1_cond;
}
wloop_1_after: ...
```

# Noteable differences from HIR
Once represented in MIR, the tree will have no notion of "paths", everything will have already been fully converted to node references at
HIR, and once we are in MIR, the plan is to have everything almost completely flattened into a "bytecode" like format.
