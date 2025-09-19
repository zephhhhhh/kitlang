use std::{cell::RefCell, collections::HashMap, sync::Arc};

use crate::{
    ast::{BinaryOpKind, Literal, UnaryOpKind},
    intermediate::{
        hir::{
            HLIR, HirId, OwnerDefId,
            exprs::{Expr, ExprKind, RefPath, ResolvedID},
            nodes::{Block, HIRNode},
            statements::{Statement, StatementKind},
        },
        resolver::{Namespace, NamespaceKind},
    },
};

#[derive(Debug, Clone)]
pub struct Program {
    pub namespace: Namespace,
    pub tree: HLIR,
}

pub type ProgramType = Program;

#[derive(Default, Debug, Clone, PartialEq)]
pub enum Value {
    #[default]
    Unit,
    Integer(i64),
    UnsignedInteger(u64),
    Float(f64),
    String(String),
    Boolean(bool),
    Reference(ResolvedID),
}

impl Value {
    pub fn from_literal(lit: Literal) -> Self {
        match lit {
            Literal::String(s) => Self::String(s),
            Literal::Float(f) => Self::Float(f),
            Literal::Integer(i) => Self::Integer(i),
            Literal::Boolean(b) => Self::Boolean(b),
        }
    }

    pub fn is_unit(&self) -> bool {
        matches!(self, Self::Unit)
    }

    pub fn repr_string(&self) -> String {
        match self {
            Value::Unit => "()".to_string(),
            Value::Integer(i) => i.to_string(),
            Value::UnsignedInteger(u) => u.to_string(),
            Value::Float(f) => f.to_string(),
            Value::String(s) => s.clone(),
            Value::Boolean(b) => b.to_string(),
            Value::Reference(resolved_id) => format!("Ref({:?})", resolved_id),
        }
    }

    pub fn int(&self) -> Option<i64> {
        match self {
            Value::Integer(i) => Some(*i),
            _ => None,
        }
    }

    pub fn uint(&self) -> Option<u64> {
        match self {
            Value::UnsignedInteger(i) => Some(*i),
            _ => None,
        }
    }

    pub fn float(&self) -> Option<f64> {
        match self {
            Value::Float(i) => Some(*i),
            _ => None,
        }
    }

    pub fn string(&self) -> Option<String> {
        match self {
            Value::String(s) => Some(s.clone()),
            _ => None,
        }
    }

    pub fn str_ref(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(&s),
            _ => None,
        }
    }

    pub fn bool(&self) -> Option<bool> {
        match self {
            Value::Boolean(b) => Some(*b),
            _ => None,
        }
    }

    pub fn are_matching_types(&self, other: &Self) -> bool {
        match self {
            Value::Unit => matches!(other, Self::Unit),
            Value::Integer(_) => matches!(other, Self::Integer(_)),
            Value::Float(_) => matches!(other, Self::Float(_)),
            Value::String(_) => matches!(other, Self::String(_)),
            Value::Boolean(_) => matches!(other, Self::Boolean(_)),
            _ => false,
        }
    }

    pub fn perform_unary_op(&self, op: UnaryOpKind) -> Option<Self> {
        match self {
            Value::Unit => None,
            Value::Integer(i) => match op {
                UnaryOpKind::Dereference => todo!(),
                UnaryOpKind::Not => Some(Value::Integer(!i)),
                UnaryOpKind::Negate => Some(Value::Integer(-i)),
            },
            Value::UnsignedInteger(i) => match op {
                UnaryOpKind::Dereference => todo!(),
                UnaryOpKind::Not => Some(Value::UnsignedInteger(!i)),
                UnaryOpKind::Negate => None,
            },
            Value::Float(f) => match op {
                UnaryOpKind::Dereference => todo!(),
                UnaryOpKind::Not => None,
                UnaryOpKind::Negate => Some(Value::Float(-f)),
            },
            Value::String(_s) => match op {
                UnaryOpKind::Dereference => todo!(),
                UnaryOpKind::Not => None,
                UnaryOpKind::Negate => None,
            },
            Value::Boolean(b) => match op {
                UnaryOpKind::Dereference => todo!(),
                UnaryOpKind::Not => Some(Value::Boolean(!b)),
                UnaryOpKind::Negate => None,
            },
            Value::Reference(_) => None,
        }
    }

    pub fn perform_binary_op(&self, rhs: &Self, op: BinaryOpKind) -> Option<Self> {
        if !self.are_matching_types(rhs) {
            return None;
        }

        fn perform_int_op(lhs: i64, rhs: i64, op: BinaryOpKind) -> Option<Value> {
            match op {
                BinaryOpKind::Add => Some(Value::Integer(lhs + rhs)),
                BinaryOpKind::Sub => Some(Value::Integer(lhs - rhs)),
                BinaryOpKind::Mul => Some(Value::Integer(lhs * rhs)),
                BinaryOpKind::Div => Some(Value::Integer(lhs / rhs)),
                BinaryOpKind::Mod => Some(Value::Integer(lhs % rhs)),
                BinaryOpKind::BitwiseXOR => Some(Value::Integer(lhs ^ rhs)),
                BinaryOpKind::BitwiseAND => Some(Value::Integer(lhs & rhs)),
                BinaryOpKind::BitwiseOR => Some(Value::Integer(lhs | rhs)),
                BinaryOpKind::ShiftLeft => Some(Value::Integer(lhs << rhs)),
                BinaryOpKind::ShiftRight => Some(Value::Integer(lhs >> rhs)),
                BinaryOpKind::Equal => Some(Value::Boolean(lhs == rhs)),
                BinaryOpKind::NotEqual => Some(Value::Boolean(lhs != rhs)),
                BinaryOpKind::LessThan => Some(Value::Boolean(lhs < rhs)),
                BinaryOpKind::GreaterThan => Some(Value::Boolean(lhs > rhs)),
                BinaryOpKind::LessThanOrEqual => Some(Value::Boolean(lhs <= rhs)),
                BinaryOpKind::GreaterThanOrEqual => Some(Value::Boolean(lhs >= rhs)),
                _ => None,
            }
        }

        fn perform_uint_op(lhs: u64, rhs: u64, op: BinaryOpKind) -> Option<Value> {
            match op {
                BinaryOpKind::Add => Some(Value::UnsignedInteger(lhs + rhs)),
                BinaryOpKind::Sub => Some(Value::UnsignedInteger(lhs - rhs)),
                BinaryOpKind::Mul => Some(Value::UnsignedInteger(lhs * rhs)),
                BinaryOpKind::Div => Some(Value::UnsignedInteger(lhs / rhs)),
                BinaryOpKind::Mod => Some(Value::UnsignedInteger(lhs % rhs)),
                BinaryOpKind::BitwiseXOR => Some(Value::UnsignedInteger(lhs ^ rhs)),
                BinaryOpKind::BitwiseAND => Some(Value::UnsignedInteger(lhs & rhs)),
                BinaryOpKind::BitwiseOR => Some(Value::UnsignedInteger(lhs | rhs)),
                BinaryOpKind::ShiftLeft => Some(Value::UnsignedInteger(lhs << rhs)),
                BinaryOpKind::ShiftRight => Some(Value::UnsignedInteger(lhs >> rhs)),
                BinaryOpKind::Equal => Some(Value::Boolean(lhs == rhs)),
                BinaryOpKind::NotEqual => Some(Value::Boolean(lhs != rhs)),
                BinaryOpKind::LessThan => Some(Value::Boolean(lhs < rhs)),
                BinaryOpKind::GreaterThan => Some(Value::Boolean(lhs > rhs)),
                BinaryOpKind::LessThanOrEqual => Some(Value::Boolean(lhs <= rhs)),
                BinaryOpKind::GreaterThanOrEqual => Some(Value::Boolean(lhs >= rhs)),
                _ => None,
            }
        }

        fn perform_float_op(lhs: f64, rhs: f64, op: BinaryOpKind) -> Option<Value> {
            match op {
                BinaryOpKind::Add => Some(Value::Float(lhs + rhs)),
                BinaryOpKind::Sub => Some(Value::Float(lhs - rhs)),
                BinaryOpKind::Mul => Some(Value::Float(lhs * rhs)),
                BinaryOpKind::Div => Some(Value::Float(lhs / rhs)),
                BinaryOpKind::Mod => Some(Value::Float(lhs % rhs)),
                BinaryOpKind::Equal => Some(Value::Boolean(lhs == rhs)),
                BinaryOpKind::NotEqual => Some(Value::Boolean(lhs != rhs)),
                BinaryOpKind::LessThan => Some(Value::Boolean(lhs < rhs)),
                BinaryOpKind::GreaterThan => Some(Value::Boolean(lhs > rhs)),
                BinaryOpKind::LessThanOrEqual => Some(Value::Boolean(lhs <= rhs)),
                BinaryOpKind::GreaterThanOrEqual => Some(Value::Boolean(lhs >= rhs)),
                _ => None,
            }
        }

        fn perform_string_op(lhs: &str, rhs: &str, op: BinaryOpKind) -> Option<Value> {
            match op {
                BinaryOpKind::Add => Some(Value::String(lhs.to_string() + rhs)),
                BinaryOpKind::Equal => Some(Value::Boolean(lhs == rhs)),
                BinaryOpKind::NotEqual => Some(Value::Boolean(lhs != rhs)),
                BinaryOpKind::LessThan => Some(Value::Boolean(lhs < rhs)),
                BinaryOpKind::GreaterThan => Some(Value::Boolean(lhs > rhs)),
                BinaryOpKind::LessThanOrEqual => Some(Value::Boolean(lhs <= rhs)),
                BinaryOpKind::GreaterThanOrEqual => Some(Value::Boolean(lhs >= rhs)),
                _ => None,
            }
        }

        fn perform_bool_op(lhs: bool, rhs: bool, op: BinaryOpKind) -> Option<Value> {
            match op {
                BinaryOpKind::Equal => Some(Value::Boolean(lhs == rhs)),
                BinaryOpKind::NotEqual => Some(Value::Boolean(lhs != rhs)),
                BinaryOpKind::LessThan => Some(Value::Boolean(lhs < rhs)),
                BinaryOpKind::GreaterThan => Some(Value::Boolean(lhs > rhs)),
                BinaryOpKind::LessThanOrEqual => Some(Value::Boolean(lhs <= rhs)),
                BinaryOpKind::GreaterThanOrEqual => Some(Value::Boolean(lhs >= rhs)),
                _ => None,
            }
        }

        match self {
            Value::Unit => None,
            Value::Integer(i) => perform_int_op(*i, rhs.int()?, op),
            Value::UnsignedInteger(u) => perform_uint_op(*u, rhs.uint()?, op),
            Value::Float(f) => perform_float_op(*f, rhs.float()?, op),
            Value::String(s) => perform_string_op(s, rhs.str_ref()?, op),
            Value::Boolean(b) => perform_bool_op(*b, rhs.bool()?, op),
            Value::Reference(_) => None,
        }
    }
}

impl From<Literal> for Value {
    fn from(value: Literal) -> Self {
        Self::from_literal(value)
    }
}

impl From<&Literal> for Value {
    fn from(value: &Literal) -> Self {
        Self::from_literal(value.clone())
    }
}

macro_rules! impl_value_ty {
    ($variant: ident, $int_ty: ty) => {
        impl From<$int_ty> for Value {
            fn from(value: $int_ty) -> Self {
                Self::$variant(value.into())
            }
        }
    };
    ($variant: ident, $int_ty: ty, $($rest:ty),+) => {
        impl_value_ty!($variant, $int_ty);
        impl_value_ty!($variant, $($rest),*);
    };
}

impl_value_ty!(Integer, i64, i32, i16, i8);
impl_value_ty!(UnsignedInteger, u64, u32, u16, u8);
impl_value_ty!(Boolean, bool);
impl_value_ty!(Float, f64, f32);

impl From<&str> for Value {
    fn from(value: &str) -> Self {
        Self::String(value.to_string())
    }
}

impl From<String> for Value {
    fn from(value: String) -> Self {
        Self::String(value)
    }
}

impl From<()> for Value {
    fn from(_: ()) -> Self {
        Self::Unit
    }
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct InterpreterPosition {
    pub block: HirId,
    pub statement: u32,
}

impl InterpreterPosition {
    pub fn new(block_id: HirId, statement_index: u32) -> Self {
        Self {
            block: block_id,
            statement: statement_index,
        }
    }

    pub fn new_invalid() -> Self {
        Self::new(HirId::PLACEHOLDER_ID, u32::MAX)
    }

    pub fn is_invalid(&self) -> bool {
        self.block == HirId::PLACEHOLDER_ID || self.statement == u32::MAX
    }

    pub fn from_block(id: HirId) -> Self {
        Self::new(id, 0)
    }
}

pub type KitlangNativeFn = dyn Fn(&mut InterpreterState, &[Value]) -> Option<Value> + Send + Sync;

pub trait IntoKitlangFn {
    fn into_kitlang_fn(self) -> Arc<KitlangNativeFn>;
}

pub trait IntoReturn {
    fn into_kitlang_return(self) -> Option<Value>;
}

impl<T: Into<Value>> IntoReturn for T {
    fn into_kitlang_return(self) -> Option<Value> {
        Some(self.into())
    }
}

impl<T: Into<Value>> IntoReturn for Option<T> {
    fn into_kitlang_return(self) -> Option<Value> {
        Some(self?.into())
    }
}

impl<R, F> IntoKitlangFn for F
where
    F: Fn(&mut InterpreterState, &[Value]) -> R + Send + Sync + 'static,
    R: IntoReturn,
{
    fn into_kitlang_fn(self) -> Arc<KitlangNativeFn> {
        Arc::new(move |state, args| self(state, args).into_kitlang_return())
    }
}

#[derive(Clone)]
pub struct InterpreterState {
    pub entry: OwnerDefId,
    pub position_stack: Vec<InterpreterPosition>,
    pub scopes: Vec<HashMap<HirId, Value>>,

    pub native_functions: HashMap<String, Arc<KitlangNativeFn>>,
}

impl InterpreterState {
    const DEFAULT_ENTRY_NAME: &str = "main";

    fn find_entry_point(program: &ProgramType) -> Option<OwnerDefId> {
        let main = program.namespace.items.get(Self::DEFAULT_ENTRY_NAME)?;
        if matches!(main.kind, NamespaceKind::Function) {
            main.id.owner_def_id()
        } else {
            None
        }
    }

    pub fn new(program: &ProgramType) -> Option<Self> {
        let entry = Self::find_entry_point(program)?;
        Some(Self {
            entry,
            position_stack: Vec::new(),
            scopes: Vec::new(),
            native_functions: HashMap::new(),
        })
    }
}

impl InterpreterState {
    pub fn register_native_function<F: IntoKitlangFn>(&mut self, name: &str, func: F) {
        if !self.native_functions.contains_key(name) {
            self.native_functions
                .insert(name.to_string(), func.into_kitlang_fn());
        }
    }

    pub fn call_native_function(&mut self, name: &str, args: &[Value]) -> Option<Value> {
        if let Some(func) = self.native_functions.get(name).cloned() {
            func(self, args)
        } else {
            eprintln!("Failed to find native function: {}", name);
            None
        }
    }

    pub fn execute_from_entry(&mut self, program: &ProgramType) -> Option<Value> {
        self.execute_function(program, self.entry, &[])
    }
}

impl InterpreterState {
    fn push_execution_location(&mut self, pos: InterpreterPosition) {
        self.position_stack.push(pos);
    }

    fn pop_position(&mut self) {
        self.position_stack.pop();
    }

    fn current_pos_cloned(&self) -> Option<InterpreterPosition> {
        self.position_stack.last().cloned()
    }

    fn current_pos(&self) -> Option<&InterpreterPosition> {
        self.position_stack.last()
    }

    fn current_pos_expect(&self) -> &InterpreterPosition {
        self.current_pos().expect("Interpreter stack empty.")
    }

    fn current_pos_mut(&mut self) -> Option<&mut InterpreterPosition> {
        self.position_stack.last_mut()
    }

    fn current_pos_mut_expect(&mut self) -> &mut InterpreterPosition {
        self.current_pos_mut().expect("Interpreter stack empty.")
    }
}

// Implementation details..
impl InterpreterState {
    const SAFETY_MAX_TICKS: usize = 10000;

    fn get_current_block<'a>(&self, program: &'a ProgramType) -> Option<&'a Block> {
        let pos = self.current_pos()?;
        let HIRNode::Block(block) = program.tree.get_hir_node(pos.block)? else {
            return None;
        };
        Some(block)
    }

    fn get_statement<'a>(
        program: &'a ProgramType,
        block: &'a Block,
        statement_index: u32,
    ) -> Option<&'a Statement> {
        let statement_id = block.statements.get(statement_index as usize)?;
        let HIRNode::Statement(statement) = program.tree.get_hir_node(*statement_id)? else {
            return None;
        };
        Some(statement)
    }

    fn get_expr_from_id(program: &ProgramType, id: HirId) -> Option<&Expr> {
        let HIRNode::Expr(expr) = program.tree.get_hir_node(id)? else {
            return None;
        };

        Some(expr)
    }

    fn read_value(&mut self, value: Value) -> Option<Value> {
        match value {
            Value::Reference(ResolvedID::Hir(resolved_id)) => self
                .scopes
                .last()
                .expect("Scope exists.")
                .get(&resolved_id)
                .cloned(),
            a => Some(a),
        }
    }

    fn execute_function(
        &mut self,
        program: &ProgramType,
        func_id: OwnerDefId,
        args: &[Value],
    ) -> Option<Value> {
        let func = program.tree.owning_node(func_id)?.hir_function_ref()?;
        if func.native {
            self.call_native_function(func.ident.str(), args)
        } else {
            let body = func.body.as_ref()?;

            if args.len() != body.params.len() {
                eprintln!(
                    "Arg len: {}, body params: {}",
                    args.len(),
                    body.params.len()
                );
                eprintln!("Function parameter lengths don't match!");
                return None;
            }

            self.push_execution_location(InterpreterPosition::from_block(body.block));
            self.scopes.push(HashMap::new());

            for (p, a) in body.params.iter().zip(args) {
                self.scopes
                    .last_mut()
                    .expect("Scope exists.")
                    .insert(*p, a.clone());
            }

            let block = self.get_current_block(program)?;

            for _i_safety in 0..Self::SAFETY_MAX_TICKS {
                let statement_index = self.current_pos()?.statement;
                if let Some(statement) = Self::get_statement(program, block, statement_index) {
                    let (statement_value, is_return) =
                        self.execute_statement(program, statement)?;

                    if (statement_index as usize + 1) >= block.statements.len() || is_return {
                        //println!("Exiting scope: {:#?}", self.scopes.last());
                        self.pop_position();
                        self.scopes.pop();
                        return Some(statement_value);
                    } else {
                        self.current_pos_mut_expect().statement += 1;
                    }
                } else {
                    eprintln!("Failed to get statement!");
                    return None;
                }
            }

            None
        }
    }

    fn execute_expr_from_id_ext(
        &mut self,
        program: &ProgramType,
        id: HirId,
        should_read: bool,
    ) -> Option<(Value, bool)> {
        let expr = Self::get_expr_from_id(program, id)?;
        let mut do_return = false;
        let ret_val = match &expr.kind {
            ExprKind::Block(hir_id) => {
                return self.execute_block_from_id(program, *hir_id);
            }
            ExprKind::Literal(literal) => Some(literal.into()),
            ExprKind::BinaryOp(binary_op_kind, hir_id, hir_id1) => {
                let (lhs_value, _) = self.execute_expr_from_id(program, *hir_id)?;
                let (rhs_value, _) = self.execute_expr_from_id(program, *hir_id1)?;

                lhs_value.perform_binary_op(&rhs_value, *binary_op_kind)
            }
            ExprKind::UnaryOp(unary_op_kind, hir_id) => {
                let (lhs_value, _) = self.execute_expr_from_id(program, *hir_id)?;

                lhs_value.perform_unary_op(*unary_op_kind)
            }
            ExprKind::If(hir_id, hir_id1, hir_id2) => {
                let (condition_expr, _) = self.execute_expr_from_id(program, *hir_id)?;
                if let Some(if_cond) = condition_expr.bool() {
                    if if_cond {
                        Some(self.execute_block_from_id(program, *hir_id1)?.0)
                    } else if let Some(else_expr) = hir_id2 {
                        Some(self.execute_expr_from_id(program, *else_expr)?.0)
                    } else {
                        Some(Value::Unit)
                    }
                } else {
                    eprintln!("Condition expression is not a bool!");
                    None
                }
            }
            ExprKind::While(hir_id, hir_id1) => {
                const MAX_WHILE_LOOP_LIMIT: usize = 10000;
                let (mut should_loop, _) = self.execute_expr_from_id(program, *hir_id)?;
                if should_loop.bool().is_none() {
                    eprintln!("While loop condition was not boolean!");
                    return None;
                }

                let mut while_result = None;
                for _i in 0..MAX_WHILE_LOOP_LIMIT {
                    if !should_loop.bool()? {
                        break;
                    }
                    while_result = Some(self.execute_block_from_id(program, *hir_id1)?);
                    should_loop = self.execute_expr_from_id(program, *hir_id)?.0;
                }

                if while_result.is_none() {
                    while_result = Some((Value::Unit, false));
                }

                return while_result;
            }
            ExprKind::Assign(hir_id, hir_id1) => {
                let (to_assign, _) = self.execute_expr_from_id_ext(program, *hir_id, false)?;

                if let Value::Reference(ResolvedID::Hir(hir_id)) = to_assign {
                    let (eval_target_expr, _) = self.execute_expr_from_id(program, *hir_id1)?;
                    let new_value = self.read_value(eval_target_expr)?;
                    if let Some(assign_target) = self
                        .scopes
                        .last_mut()
                        .expect("Scope doesn't exist?")
                        .get_mut(&hir_id)
                    {
                        *assign_target = new_value.clone();
                        Some(new_value)
                    } else {
                        eprintln!("Failed to get assign target: {:?}", hir_id);
                        None
                    }
                } else {
                    eprintln!("Invalid assign target! {:?}", to_assign);
                    None
                }
            }
            ExprKind::Call(hir_id, hir_ids) => {
                let (call_target, _) = self.execute_expr_from_id(program, *hir_id)?;
                if let Value::Reference(ResolvedID::OwnerDef(id)) = call_target {
                    let args: Option<Vec<Value>> = hir_ids
                        .iter()
                        .map(|id| Some(self.execute_expr_from_id(program, *id)?.0))
                        .collect();
                    if let Some(args) = args {
                        self.execute_function(program, id, &args)
                    } else {
                        eprintln!("Failed to evaluate all function arguments!");
                        None
                    }
                } else {
                    eprintln!("Invalid call target: {:?}", call_target);
                    None
                }
            }
            ExprKind::MethodCall(hir_id, ident, hir_ids) => todo!(),
            ExprKind::Index(hir_id, hir_id1) => todo!(),
            ExprKind::FieldAccess(hir_id, ident) => todo!(),
            ExprKind::StructInit(struct_initialisation) => todo!(),
            ExprKind::Path(RefPath::Resolved(ref_path, resolved_id)) => {
                let value = Value::Reference(*resolved_id);
                if should_read {
                    self.read_value(value)
                } else {
                    Some(value)
                }
            }
            ExprKind::Path(RefPath::Unresolved(path)) => {
                eprintln!("Error: Unresolved path: {:?}", path);
                None
            }
            ExprKind::Continue => todo!(),
            ExprKind::Break => todo!(),
            ExprKind::Return(hir_id) => {
                do_return = true;
                Some(if let Some(ret_id) = hir_id {
                    self.execute_expr_from_id(program, *ret_id)?.0
                } else {
                    Value::Unit
                })
            }
        };

        Some((ret_val?, do_return))
    }

    fn execute_expr_from_id(&mut self, program: &ProgramType, id: HirId) -> Option<(Value, bool)> {
        self.execute_expr_from_id_ext(program, id, true)
    }

    fn execute_block_from_id(&mut self, program: &ProgramType, id: HirId) -> Option<(Value, bool)> {
        let HIRNode::Block(block) = program.tree.get_hir_node(id)? else {
            return None;
        };

        for statement_index in 0..block.statements.len() {
            if let Some(statement) = Self::get_statement(program, block, statement_index as u32) {
                let (statement_value, is_return) = self.execute_statement(program, statement)?;

                if is_return {
                    return Some((statement_value, true));
                } else if (statement_index + 1) >= block.statements.len() {
                    return Some((statement_value, false));
                }
            } else {
                eprintln!("Failed to get statement!");
                return None;
            }
        }

        None
    }

    fn execute_statement(
        &mut self,
        program: &ProgramType,
        statement: &Statement,
    ) -> Option<(Value, bool)> {
        match &statement.kind {
            StatementKind::Let(let_statement) => {
                if let Some(init_expr) = let_statement.initial_value {
                    let value = self.execute_expr_from_id(program, init_expr)?;
                    self.scopes
                        .last_mut()
                        .expect("Scope exists.")
                        .insert(statement.id, value.0.clone());
                    return Some(value);
                }
                Some((Value::Unit, false))
            }
            StatementKind::Item(_) => Some((Value::Unit, false)),
            StatementKind::Expr(id) => self.execute_expr_from_id(program, *id),
            StatementKind::Semi(id) => {
                self.execute_expr_from_id(program, *id);
                Some((Value::Unit, false))
            }
        }
    }
}

#[derive(Clone, Default)]
pub struct Interpreter {
    pub program: Option<RefCell<ProgramType>>,
    pub state: Option<InterpreterState>,
}

impl Interpreter {
    pub fn load_program(&mut self, program: ProgramType) -> Option<()> {
        self.state = Some(InterpreterState::new(&program)?);
        self.program = Some(RefCell::new(program));

        Some(())
    }

    pub fn register_native_function<F: IntoKitlangFn>(&mut self, name: &str, func: F) {
        if let Some(state) = self.state.as_mut() {
            state.register_native_function(name, func);
        }
    }

    pub fn execute_from_entry(&mut self) -> Option<Value> {
        if let Some(state) = self.state.as_mut() {
            if let Some(program) = self.program.as_ref() {
                let program_ref = program.borrow();
                return state.execute_from_entry(&program_ref);
            }
        }

        None
    }
}
