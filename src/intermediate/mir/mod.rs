use crate::ast::{BinaryOpKind, Mutability, SourceSpan, UnaryOpKind};

#[derive(Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BasicBlockId(pub u32);

impl BasicBlockId {
    pub const PLACEHOLDER_ID: BasicBlockId = BasicBlockId(u32::MAX);

    pub fn is_placeholder(self) -> bool {
        self == Self::PLACEHOLDER_ID
    }
}

impl ::std::fmt::Debug for BasicBlockId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "BasicBlockId({})", self.0)
    }
}

#[derive(Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LocalId(pub u32);

impl LocalId {
    pub const PLACEHOLDER_ID: LocalId = LocalId(u32::MAX);
    pub const RETURN_VALUE: LocalId = LocalId(0);

    pub fn is_placeholder(self) -> bool {
        self == Self::PLACEHOLDER_ID
    }

    pub fn is_return_value(self) -> bool {
        self == Self::RETURN_VALUE
    }
}

impl ::std::fmt::Debug for LocalId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "LocalId({})", self.0)
    }
}


#[derive(Debug, Clone)]
pub enum LocalInfo {
    UserDeclared/*(SourceSpan)*/,
    Temp,
}

#[derive(Debug, Clone)]
pub struct LocalDefinition {
    pub mutable: Mutability,
    pub info: LocalInfo,
    // pub ty: TODO..
}

#[derive(Debug, Clone)]
pub enum Operand {
    Copy(LocalId),
    Const // Not sure yet
}

#[derive(Debug, Clone)]
pub enum RValue {
    Unchanged(Operand),
    Ref(LocalId),
    BinaryOp(BinaryOpKind, (Operand, Operand)),
    UnaryOp(UnaryOpKind, Operand),
}

#[derive(Debug, Clone)]
pub enum StatementKind {
    Assign(LocalId, RValue),
}

#[derive(Debug, Clone)]
pub struct Statement {
    pub kind: StatementKind,
}

#[derive(Debug, Clone)]
pub enum BlockExitKind {
    Goto(BasicBlockId),
    Branch(LocalId, BasicBlockId, BasicBlockId),
    Return,
    Call/*(target, args)*/,
}

#[derive(Debug, Clone)]
pub struct ExitDirective {
    pub kind: BlockExitKind,
}

#[derive(Debug, Clone)]
pub struct BasicBlock {
    pub statements: Vec<Statement>,
    pub exit_directive: ExitDirective,
}

#[derive(Debug, Clone)]
pub struct Body {
    // locals[0] is _ALWAYS_ the return value.
    pub locals: Vec<LocalDefinition>,
    pub blocks: Vec<BasicBlock>,
    pub arg_count: u32,
}

impl Body {
    pub fn new(params: &[LocalDefinition]) -> Self {
        let mut locals = vec![LocalDefinition {
            mutable: Mutability::Mutable,
            info: LocalInfo::Temp
        }];
        locals.extend_from_slice(params);
        Self {
            locals,
            blocks: Vec::new(),
            arg_count: params.len() as u32,
        }
    }
}

impl Body {
    pub fn local(&self, id: LocalId) -> Option<&LocalDefinition> {
        self.locals.get(id.0 as usize)
    }

    pub fn local_mut(&mut self, id: LocalId) -> Option<&mut LocalDefinition> {
        self.locals.get_mut(id.0 as usize)
    }

    pub fn block(&self, id: BasicBlockId) -> Option<&BasicBlock> {
        self.blocks.get(id.0 as usize)
    }

    pub fn block_mut(&mut self, id: BasicBlockId) -> Option<&mut BasicBlock> {
        self.blocks.get_mut(id.0 as usize)
    }
}





mod mir_impl {
    use crate::intermediate::{hir::{errors::*, nodes::{Block, Function, HIRNode, OwningNodeKind}, HLIR}, mir::{Body, LocalDefinition, LocalInfo}};

    pub fn lower_hir_to_mir(hlir: &HLIR) -> LowerResult<()> {
        for i in hlir.owner_id_iter() {
            if let Some(node) = hlir.owning_node(i) {
                if let Some(func) = node.hir_function_ref() {
                    lower_hir_fn_to_mir(hlir, func).unwrap();
                }
            }
        }

        Ok(())
    }

    pub fn lower_hir_fn_to_mir(hlir: &HLIR, func: &Function) -> LowerResult<()> {
        if let Some(body) = &func.body {
            let params: Option<Vec<_>> = body.params.iter().map(|p| {
                if let HIRNode::Param(p) = hlir.get_hir_node(*p)? {
                    Some(p.mutable)
                } else {
                    None
                }
            }).collect();
            if let Some(HIRNode::Block(b)) = hlir.get_hir_node(body.block) {
                let mir_params: Vec<_> = params.unwrap().iter().map(|p| LocalDefinition {
                    mutable: *p,
                    info: LocalInfo::UserDeclared,
                }).collect();
                let mut body = Body::new(&mir_params);

                
            }
        }

        Ok(())
    }
}


pub fn lower_hir_fn_to_mir(hlir: &crate::intermediate::hir::HLIR) -> crate::intermediate::hir::errors::LowerResult<()> {
    mir_impl::lower_hir_to_mir(hlir)
}
