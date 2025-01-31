use std::iter::Peekable;
use std::rc::Rc;

use anyhow::{Context as _, Result};

use crate::core::*;

pub use self::arithmetic::Arithmetic;
pub use self::cell_utils::CellUtils;
pub use self::control::Control;
pub use self::crypto::Crypto;
pub use self::debug_utils::DebugUtils;
pub use self::dict_utils::DictUtils;
pub use self::stack_utils::StackUtils;
pub use self::string_utils::StringUtils;
pub use self::vm_utils::VmUtils;

mod arithmetic;
mod cell_utils;
mod control;
mod crypto;
mod debug_utils;
mod dict_utils;
mod stack_utils;
mod string_utils;
mod vm_utils;

pub struct BaseModule;

#[fift_module]
impl FiftModule for BaseModule {
    #[init]
    fn init(&self, d: &mut Dictionary) -> Result<()> {
        d.define_word("nop", cont::NopCont::instance())
    }

    #[cmd(name = "null", stack)]
    fn interpret_push_null(stack: &mut Stack) -> Result<()> {
        stack.push_null()
    }

    #[cmd(name = "null?", stack, args(ty = StackValueType::Null))]
    #[cmd(name = "integer?", stack, args(ty = StackValueType::Int))]
    #[cmd(name = "string?", stack, args(ty = StackValueType::String))]
    #[cmd(name = "tuple?", stack, args(ty = StackValueType::Tuple))]
    #[cmd(name = "box?", stack, args(ty = StackValueType::SharedBox))]
    #[cmd(name = "atom?", stack, args(ty = StackValueType::Atom))]
    fn interpret_is_type(stack: &mut Stack, ty: StackValueType) -> Result<()> {
        let is_ty = stack.pop()?.ty() == ty;
        stack.push_bool(is_ty)
    }

    #[cmd(name = "hole", stack)]
    fn interpret_hole(stack: &mut Stack) -> Result<()> {
        stack.push(SharedBox::default())
    }

    #[cmd(name = "box", stack)]
    fn interpret_box(stack: &mut Stack) -> Result<()> {
        let value = stack.pop()?;
        stack.push(SharedBox::new(value))
    }

    #[cmd(name = "@", stack)]
    fn interpret_box_fetch(stack: &mut Stack) -> Result<()> {
        let value = stack.pop_shared_box()?;
        stack.push_raw(value.fetch())
    }

    #[cmd(name = "!", stack)]
    fn interpret_box_store(stack: &mut Stack) -> Result<()> {
        let value = stack.pop_shared_box()?;
        value.store(stack.pop()?);
        Ok(())
    }

    #[cmd(name = "anon", stack)]
    fn interpret_atom_anon(stack: &mut Stack) -> Result<()> {
        let anon = stack.atoms_mut().create_anon();
        stack.push(anon)
    }

    #[cmd(name = "(atom)", stack)]
    fn interpret_atom(stack: &mut Stack) -> Result<()> {
        let create = stack.pop_bool()?;
        let name = stack.pop_string()?;
        let mut atom = stack.atoms().get(&*name);
        if create && atom.is_none() {
            atom = Some(stack.atoms_mut().create_named(&*name));
        }
        let exists = atom.is_some();
        if let Some(atom) = atom {
            stack.push(atom)?;
        }
        stack.push_bool(exists)
    }

    #[cmd(name = "atom>$", stack)]
    fn interpret_atom_name(stack: &mut Stack) -> Result<()> {
        let atom = stack.pop_atom()?;
        stack.push(atom.to_string())
    }

    #[cmd(name = "eq?", stack)]
    fn interpret_is_eq(stack: &mut Stack) -> Result<()> {
        let y = stack.pop()?;
        let x = stack.pop()?;
        stack.push_bool(x.is_equal(&*y))
    }

    #[cmd(name = "eqv?", stack)]
    fn interpret_is_eqv(stack: &mut Stack) -> Result<()> {
        let y = stack.pop()?;
        let x = stack.pop()?;
        let ty = x.ty();

        stack.push_bool(if ty == y.ty() {
            match ty {
                StackValueType::Null => true,
                StackValueType::Atom => *x.as_atom()? == *y.as_atom()?,
                StackValueType::Int => *x.as_int()? == *y.as_int()?,
                StackValueType::String => x.as_string()? == y.as_string()?,
                _ => false,
            }
        } else {
            false
        })
    }

    #[cmd(name = "|", stack)]
    fn interpret_empty_tuple(stack: &mut Stack) -> Result<()> {
        stack.push(StackTuple::new())
    }

    #[cmd(name = ",", stack)]
    fn interpret_tuple_push(stack: &mut Stack) -> Result<()> {
        let value = stack.pop()?;
        let mut tuple = stack.pop_tuple()?;
        Rc::make_mut(&mut tuple).push(value);
        stack.push_raw(tuple)
    }

    #[cmd(name = "tpop", stack)]
    fn interpret_tuple_pop(stack: &mut Stack) -> Result<()> {
        let mut tuple = stack.pop_tuple()?;
        let last = Rc::make_mut(&mut tuple).pop().context("Tuple underflow")?;
        stack.push_raw(tuple)?;
        stack.push_raw(last)
    }

    #[cmd(name = "[]", stack)]
    fn interpret_tuple_index(stack: &mut Stack) -> Result<()> {
        let idx = stack.pop_usize()?;
        let tuple = stack.pop_tuple()?;
        let value = tuple
            .get(idx)
            .with_context(|| format!("Index {idx} is out of the tuple range"))?
            .clone();
        stack.push_raw(value)
    }

    #[cmd(name = "[]=", stack)]
    fn interpret_tuple_set(stack: &mut Stack) -> Result<()> {
        let idx = stack.pop_usize()?;
        let value = stack.pop()?;
        let mut tuple = stack.pop_tuple()?;
        *Rc::make_mut(&mut tuple)
            .get_mut(idx)
            .with_context(|| format!("Index {idx} is out of the tuple range"))? = value;
        stack.push_raw(tuple)
    }

    #[cmd(name = "[]!", stack)] // []! (t v i -- t')
    fn interpret_tuple_insert(stack: &mut Stack) -> Result<()> {
        let idx = stack.pop_usize()?;
        let value = stack.pop()?;
        let mut tuple = stack.pop_tuple()?;

        let l = tuple.len();
        anyhow::ensure!(
            idx <= l,
            format!("insertion index (is {idx}) should be <= len (is {l})")
        );

        Rc::make_mut(&mut tuple).insert(idx, value);
        stack.push_raw(tuple)
    }

    #[cmd(name = "[]>$", stack, args(pop_sep = false))] //  []>$   (t[S0, S1, ..., Sn]   -- S)
    #[cmd(name = "[]>$by", stack, args(pop_sep = true))] // []>$by (t[S0, S1, ..., Sn] s -- S)
    fn interpret_tuple_strings_join(stack: &mut Stack, pop_sep: bool) -> Result<()> {
        let sep = if pop_sep {
            Some(stack.pop_string()?)
        } else {
            None
        };
        let tuple = stack.pop_tuple_owned()?;

        let mut result = String::new();

        let mut first = true;
        for item in tuple {
            if let Some(sep) = sep.as_deref() {
                if !std::mem::take(&mut first) {
                    result.push_str(sep);
                }
            }
            result.push_str(item.as_string()?);
        }

        stack.push(result)
    }

    #[cmd(name = "count", stack)]
    fn interpret_tuple_len(stack: &mut Stack) -> Result<()> {
        let len = stack.pop_tuple()?.len();
        stack.push_int(len)
    }

    #[cmd(name = "tuple", stack)]
    fn interpret_make_tuple(stack: &mut Stack) -> Result<()> {
        let n = stack.pop_smallint_range(0, 255)? as usize;
        let mut tuple = Vec::with_capacity(n);
        for _ in 0..n {
            tuple.push(stack.pop()?);
        }
        tuple.reverse();
        stack.push(tuple)
    }

    #[cmd(name = "untuple", stack, args(pop_count = true))]
    #[cmd(name = "explode", stack, args(pop_count = false))]
    fn interpret_tuple_explode(stack: &mut Stack, pop_count: bool) -> Result<()> {
        let mut n = if pop_count {
            stack.pop_smallint_range(0, 255)? as usize
        } else {
            0
        };
        let tuple = stack.pop_tuple_owned()?;
        if !pop_count {
            n = tuple.len();
            anyhow::ensure!(n <= 255, "Cannot untuple a tuple with {n} items");
        } else {
            anyhow::ensure!(
                tuple.len() == n,
                "Tuple size mismatch. Expected: {n}, actual: {}",
                tuple.len()
            );
        }

        for item in tuple {
            stack.push_raw(item)?;
        }

        if !pop_count {
            stack.push_int(n)?;
        }

        Ok(())
    }

    #[cmd(name = "allot", stack)]
    fn interpret_allot(stack: &mut Stack) -> Result<()> {
        let n = stack.pop_smallint_range(0, u32::MAX)?;
        let mut tuple = Vec::<Rc<dyn StackValue>>::new();
        tuple.resize_with(n as usize, || Rc::new(SharedBox::default()));
        stack.push(tuple)
    }

    // === Hashmaps ===

    #[cmd(name = "hmapnew", stack)]
    fn interpret_hmap_new(stack: &mut Stack) -> Result<()> {
        stack.push_null()
    }

    #[cmd(name = "hmap@", stack, args(chk = false))]
    #[cmd(name = "hmap@?", stack, args(chk = true))]
    fn interpret_hmap_fetch(stack: &mut Stack, chk: bool) -> Result<()> {
        let map = stack.pop_hashmap()?;
        let key = HashMapTreeKey::new(stack.pop()?)?;
        let value = HashMapTreeNode::lookup(&map, key).map(|node| node.value.clone());

        let found = value.is_some();
        match value {
            Some(value) => stack.push_raw(value)?,
            None if !chk => stack.push_null()?,
            _ => {}
        }
        if chk {
            stack.push_bool(found)?;
        }
        Ok(())
    }

    #[cmd(name = "hmap-", stack, args(chk = false, read = false))]
    #[cmd(name = "hmap-?", stack, args(chk = true, read = false))]
    #[cmd(name = "hmap@-", stack, args(chk = false, read = true))]
    fn interpret_hmap_delete(stack: &mut Stack, chk: bool, read: bool) -> Result<()> {
        let mut map = stack.pop_hashmap()?;
        let key = HashMapTreeKey::new(stack.pop()?)?;
        let value = HashMapTreeNode::remove(&mut map, key);
        stack.push_opt_raw(map)?;

        let exists = value.is_some();
        match value {
            Some(value) if read => stack.push_raw(value)?,
            None if read && !chk => stack.push_null()?,
            _ => {}
        }
        if chk {
            stack.push_bool(exists)?;
        }
        Ok(())
    }

    #[cmd(name = "hmap!", stack, args(add = false))]
    #[cmd(name = "hmap!+", stack, args(add = true))]
    fn interpret_hmap_store(stack: &mut Stack, add: bool) -> Result<()> {
        let mut map = stack.pop_hashmap()?;
        let key = HashMapTreeKey::new(stack.pop()?)?;
        let value = stack.pop()?;

        if add {
            HashMapTreeNode::set(&mut map, &key, &value);
        } else {
            HashMapTreeNode::replace(&mut map, key, &value);
        }
        stack.push_opt_raw(map)
    }

    #[cmd(name = "hmapempty?", stack)]
    fn interpret_hmap_is_empty(stack: &mut Stack) -> Result<()> {
        let map = stack.pop_hashmap()?;
        stack.push_bool(map.is_none())
    }

    #[cmd(name = "hmapunpack", stack)]
    fn interpret_hmap_decompose(stack: &mut Stack) -> Result<()> {
        let map = stack.pop_hashmap()?;
        let not_empty = map.is_some();

        if let Some(map) = map {
            stack.push_raw(map.key.stack_value.clone())?;
            stack.push_raw(map.value.clone())?;
            stack.push_opt_raw(map.left.clone())?;
            stack.push_opt_raw(map.right.clone())?;
        }

        stack.push_bool(not_empty)
    }

    #[cmd(name = "hmapforeach", tail)]
    fn interpret_hmap_foreach(ctx: &mut Context) -> Result<Option<Cont>> {
        let func = ctx.stack.pop_cont_owned()?;
        let Some(map) = ctx.stack.pop_hashmap()? else {
            return Ok(None);
        };
        Ok(Some(Rc::new(cont::LoopCont::new(
            HmapIterCont {
                iter: map.owned_iter().peekable(),
                ok: true,
            },
            func,
            ctx.next.take(),
        ))))
    }

    // === Environment ===

    #[cmd(name = "now")]
    fn interpret_now(ctx: &mut Context) -> Result<()> {
        ctx.stack.push_int(ctx.env.now_ms() / 1000)
    }

    #[cmd(name = "now_ms")]
    fn interpret_now_ms(ctx: &mut Context) -> Result<()> {
        ctx.stack.push_int(ctx.env.now_ms())
    }

    #[cmd(name = "getenv")]
    fn interpret_getenv(ctx: &mut Context) -> Result<()> {
        let name = ctx.stack.pop_string()?;
        let value = ctx.env.get_env(&name).unwrap_or_default();
        ctx.stack.push(value)
    }

    #[cmd(name = "getenv?")]
    fn interpret_getenv_exists(ctx: &mut Context) -> Result<()> {
        let name = ctx.stack.pop_string()?;
        let exists = match ctx.env.get_env(&name) {
            Some(value) => {
                ctx.stack.push(value)?;
                true
            }
            None => false,
        };
        ctx.stack.push_bool(exists)
    }

    #[cmd(name = "file>B")]
    fn interpret_read_file(ctx: &mut Context) -> Result<()> {
        let name = ctx.stack.pop_string()?;
        let data = ctx.env.read_file(name.as_str())?;
        ctx.stack.push(data)
    }

    #[cmd(name = "filepart>B")]
    fn interpret_read_file_part(ctx: &mut Context) -> Result<()> {
        let size = ctx.stack.pop_usize()? as u64;
        let offset = ctx.stack.pop_usize()? as u64;
        let name = ctx.stack.pop_string()?;
        let data = ctx.env.read_file_part(name.as_str(), offset, size)?;
        ctx.stack.push(data)
    }

    #[cmd(name = "B>file")]
    fn interpret_write_file(ctx: &mut Context) -> Result<()> {
        let name = ctx.stack.pop_string()?;
        let data = ctx.stack.pop_bytes()?;
        ctx.env.write_file(name.as_str(), data.as_slice())?;
        Ok(())
    }

    #[cmd(name = "file-exists?")]
    fn interpret_file_exists(ctx: &mut Context) -> Result<()> {
        let name = ctx.stack.pop_string()?;
        let exists = ctx.env.file_exists(&name);
        ctx.stack.push_bool(exists)
    }
}

#[derive(Clone)]
struct HmapIterCont {
    iter: Peekable<stack::HashMapTreeOwnedIter>,
    ok: bool,
}

impl cont::LoopContImpl for HmapIterCont {
    fn pre_exec(&mut self, ctx: &mut Context) -> Result<bool> {
        let entry = match self.iter.next() {
            Some(entry) => entry,
            None => return Ok(false),
        };

        ctx.stack.push_raw(entry.key.stack_value.clone())?;
        ctx.stack.push_raw(entry.value.clone())?;
        Ok(true)
    }

    fn post_exec(&mut self, ctx: &mut Context) -> Result<bool> {
        self.ok = ctx.stack.pop_bool()?;
        Ok(self.ok && self.iter.peek().is_some())
    }

    fn finalize(&mut self, ctx: &mut Context) -> Result<bool> {
        ctx.stack.push_bool(self.ok)?;
        Ok(true)
    }
}
