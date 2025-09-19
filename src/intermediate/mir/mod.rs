use crate::{
    ast::{BinaryOpKind, Ident, Literal, Mutability, UnaryOpKind},
    intermediate::hir::OwnerDefId,
};

#[derive(Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BasicBlockId(pub u32);

impl BasicBlockId {
    pub const PLACEHOLDER_ID: Self = Self(u32::MAX);

    pub fn is_placeholder(self) -> bool {
        self == Self::PLACEHOLDER_ID
    }

    pub fn next(self) -> Self {
        Self(self.0.saturating_add(1))
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

    pub fn next(self) -> Self {
        Self(self.0.saturating_add(1))
    }
}

impl ::std::fmt::Debug for LocalId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "LocalId({})", self.0)
    }
}

#[derive(Clone)]
pub enum LocalInfo {
    UserDeclared(Ident), /*(SourceSpan)*/
    Temp,
}

impl ::std::fmt::Debug for LocalInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UserDeclared(arg0) => write!(f, "User({})", arg0.str()),
            Self::Temp => write!(f, "Temp"),
        }
    }
}

#[derive(Clone)]
pub struct LocalDefinition {
    pub mutable: Mutability,
    pub info: LocalInfo,
    // pub ty: TODO..
}

impl ::std::fmt::Debug for LocalDefinition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "LocalDefinition{{ {:?}, {:?} }}",
            self.info, self.mutable
        )
    }
}

#[derive(Clone, PartialEq, PartialOrd)]
pub enum Operand {
    Copy(LocalId),
    Unit,
    Literal(Literal),
    Const, // Not sure yet
}

impl ::std::fmt::Debug for Operand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Copy(arg0) => write!(f, "Copy({:?})", arg0),
            Self::Unit => write!(f, "Unit"),
            Self::Literal(arg0) => arg0.fmt(f),
            Self::Const => write!(f, "Const"),
        }
    }
}

#[derive(Clone)]
pub enum RValue {
    Unchanged(Operand),
    Ref(LocalId),
    BinaryOp(BinaryOpKind, (Operand, Operand)),
    UnaryOp(UnaryOpKind, Operand),
}

impl ::std::fmt::Debug for RValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unchanged(arg0) => arg0.fmt(f),
            Self::Ref(arg0) => f.debug_tuple("Ref").field(arg0).finish(),
            Self::BinaryOp(arg0, arg1) => {
                write!(f, "BinaryOp({:?}, ({:?}, {:?}))", arg0, arg1.0, arg1.1)
            }
            Self::UnaryOp(arg0, arg1) => write!(f, "UnaryOp({:?}, {:?})", arg0, arg1),
        }
    }
}

impl RValue {
    pub fn unit() -> Self {
        Self::Unchanged(Operand::Unit)
    }

    pub fn literal(literal: Literal) -> Self {
        Self::Unchanged(Operand::Literal(literal))
    }
}

#[derive(Clone)]
pub enum StatementKind {
    Assign(LocalId, RValue),
}

impl ::std::fmt::Debug for StatementKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Assign(arg0, arg1) => write!(f, "Assign{{ {:?} = {:?} }}", arg0, arg1),
        }
    }
}

#[derive(Clone)]
pub struct Statement {
    pub kind: StatementKind,
}

impl ::std::fmt::Debug for Statement {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.kind.fmt(f)
    }
}

#[derive(Clone, PartialEq, PartialOrd)]
pub enum BlockExitKind {
    Goto(BasicBlockId),
    Branch(Operand, BasicBlockId, BasicBlockId),
    Return,
    Call(LocalId, OwnerDefId, Vec<Operand>, BasicBlockId),
}

impl ::std::fmt::Debug for BlockExitKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Goto(arg0) => write!(f, "Goto({:?})", arg0),
            Self::Branch(arg0, arg1, arg2) => f
                .debug_tuple("Branch")
                .field(arg0)
                .field(arg1)
                .field(arg2)
                .finish(),
            Self::Return => write!(f, "Return"),
            Self::Call(assign_to, def_id, args, next_block) => write!(
                f,
                "Call({:?}, {:?} -> {:?}) = {:?}",
                def_id, args, next_block, assign_to
            ),
        }
    }
}

#[derive(Clone)]
pub struct ExitDirective {
    pub kind: BlockExitKind,
}

impl ::std::fmt::Debug for ExitDirective {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ExitDirective::{:?}", self.kind)
    }
}

impl ExitDirective {
    pub fn from_kind(kind: BlockExitKind) -> Self {
        Self { kind }
    }
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
    fn get_return_slot_def() -> LocalDefinition {
        LocalDefinition {
            mutable: Mutability::Mutable,
            info: LocalInfo::Temp,
        }
    }

    pub fn new(params: &[LocalDefinition]) -> Self {
        let mut locals = vec![Self::get_return_slot_def()];
        locals.extend_from_slice(params);
        Self {
            locals,
            blocks: Vec::new(),
            arg_count: params.len() as u32,
        }
    }

    pub fn new_empty() -> Self {
        Self {
            locals: vec![Self::get_return_slot_def()],
            blocks: Vec::new(),
            arg_count: 0,
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

    pub fn block_exit_kind(&self, id: BasicBlockId) -> Option<&BlockExitKind> {
        Some(&self.block(id)?.exit_directive.kind)
    }

    pub fn block_exit_kind_mut(&mut self, id: BasicBlockId) -> Option<&mut BlockExitKind> {
        Some(&mut self.block_mut(id)?.exit_directive.kind)
    }

    pub fn next_block_id(&mut self) -> BasicBlockId {
        BasicBlockId(self.blocks.len() as u32)
    }

    pub fn next_local_id(&mut self) -> LocalId {
        LocalId(self.locals.len() as u32)
    }
}

impl Body {
    pub fn push_param(&mut self, mutable: Mutability, ident: Ident) -> LocalId {
        let def = LocalDefinition {
            mutable,
            info: LocalInfo::UserDeclared(ident),
        };
        self.arg_count += 1;
        self.push_local(def)
    }

    pub fn push_local(&mut self, def: LocalDefinition) -> LocalId {
        let id = self.locals.len() as u32;
        self.locals.push(def);
        LocalId(id)
    }

    pub fn push_block(&mut self, block: BasicBlock) -> BasicBlockId {
        let id = self.blocks.len() as u32;
        self.blocks.push(block);
        BasicBlockId(id)
    }
}

pub mod lowerer;

mod mir_impl {
    use std::collections::HashMap;

    use crate::intermediate::{
        hir::{
            HLIR, HirId,
            errors::*,
            nodes::{Block, Function, HIRNode, OwningNodeKind},
            statements::StatementKind,
        },
        mir::{Body, LocalDefinition, LocalId, LocalInfo},
    };

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
            let params: Option<Vec<_>> = body
                .params
                .iter()
                .map(|p| {
                    if let HIRNode::Param(p) = hlir.get_hir_node(*p)? {
                        Some((p.mutable, p.ident.clone()))
                    } else {
                        None
                    }
                })
                .collect();
            if let Some(HIRNode::Block(b)) = hlir.get_hir_node(body.block) {
                let mir_params: Vec<_> = params
                    .unwrap()
                    .iter()
                    .map(|(mutable, ident)| LocalDefinition {
                        mutable: *mutable,
                        info: LocalInfo::UserDeclared(ident.clone()),
                    })
                    .collect();
                let mut body = Body::new(&mir_params);
                let mut lut = HashMap::<HirId, LocalId>::new();

                for statement_id in &b.statements {
                    if let Some(HIRNode::Statement(statement)) = hlir.get_hir_node(*statement_id) {
                        match &statement.kind {
                            StatementKind::Let(let_statement) => {
                                let local_id = body.push_local(LocalDefinition {
                                    mutable: let_statement.mutable,
                                    info: LocalInfo::UserDeclared(let_statement.ident.clone()),
                                });
                                lut.insert(statement.id, local_id);
                            }
                            StatementKind::Expr(hir_id) => {}
                            StatementKind::Semi(hir_id) => {}
                            _ => {}
                        }
                    }
                }

                println!("MIR Func '{}' = {:#?}", func.ident.str(), body);
            }
        }

        Ok(())
    }
}

pub fn lower_hir_to_mir(
    hlir: &crate::intermediate::hir::HLIR,
) -> crate::intermediate::hir::errors::LowerResult<()> {
    mir_impl::lower_hir_to_mir(hlir)
}
