//! The tables no data file holds, read from the game's own classes: block hardness and
//! needs-correct-tool, collision shapes, tool materials, fuel burn times, mob health and category,
//! and the entity-data serializer ids. A JVM is started from Rust over JNI with the inner server
//! jar and the bundler's libraries on its class path, the game is bootstrapped, and its objects are
//! read by reflection, as clojurecraft's `dev/clojurecraft/jar_probe.clj` does from Clojure.
//!
//! Only `java.lang.reflect` and other JDK methods are called by JNI signature; every game method is
//! looked up by name and argument count at run time, so no game signature is typed here.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail, ensure};
use jni::objects::{GlobalRef, JObject, JObjectArray, JString, JValue};
use jni::{InitArgsBuilder, JNIEnv, JNIVersion, JavaVM};

use crate::fetch::VERSION;
use crate::model::strip_ns;

/// A block's default-state collision shape.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Collision {
    Empty,
    Full,
    /// Neither: the top of the shape, in blocks.
    Partial(f64),
}

pub struct BlockProbe {
    pub hardness: f64,
    pub needs_correct_tool: bool,
    pub collision: Collision,
}

pub struct Material {
    pub speed: f64,
    pub durability: i64,
    pub incorrect: String,
}

pub struct Mob {
    pub max_health: f64,
    pub category: String,
}

pub struct Probe {
    /// By block name (no namespace).
    pub blocks: BTreeMap<String, BlockProbe>,
    /// By the `ToolMaterial` constant's name, lower-cased (`wood`, `stone`, ...).
    pub materials: BTreeMap<String, Material>,
    /// Item name → burn ticks.
    pub fuel: BTreeMap<String, i64>,
    /// Entity type name → health and category, for the types the game gives attributes to.
    pub mobs: BTreeMap<String, Mob>,
    /// `EntityDataSerializers` constant name → its serialized id (the wire's index).
    pub serializers: BTreeMap<String, i64>,
}

/// Every `.jar` under a directory, sorted.
fn jars_under(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in std::fs::read_dir(dir).with_context(|| format!("read {}", dir.display()))? {
        let path = entry?.path();
        if path.is_dir() {
            jars_under(&path, out)?;
        } else if path.extension().is_some_and(|e| e == "jar") {
            out.push(path);
        }
    }
    Ok(())
}

/// Reflection over JNI, with the `Method`s already found kept by (class, name, arity).
struct Reflect<'a, 'e> {
    env: &'e mut JNIEnv<'a>,
    methods: BTreeMap<(String, String, usize), GlobalRef>,
}

impl<'a> Reflect<'a, '_> {
    /// Turn a JNI error into a message, printing and clearing any Java exception first.
    fn check<T>(&mut self, r: jni::errors::Result<T>, what: &str) -> Result<T> {
        r.map_err(|e| {
            if self.env.exception_check().unwrap_or(false) {
                let _ = self.env.exception_describe();
                let _ = self.env.exception_clear();
            }
            anyhow!("{what}: {e}")
        })
    }

    fn string(&mut self, s: &str) -> Result<JObject<'a>> {
        let r = self.env.new_string(s);
        Ok(JObject::from(self.check(r, "new string")?))
    }

    fn rust_string(&mut self, obj: &JObject<'a>) -> Result<String> {
        let js = JString::from(self.env.new_local_ref(obj)?);
        let r = self.env.get_string(&js).map(String::from);
        self.check(r, "read string")
    }

    fn call(
        &mut self,
        obj: &JObject<'a>,
        name: &str,
        sig: &str,
        args: &[JValue],
    ) -> Result<JObject<'a>> {
        let r = self
            .env
            .call_method(obj, name, sig, args)
            .and_then(|v| v.l());
        self.check(r, name)
    }

    fn display(&mut self, obj: &JObject<'a>) -> Result<String> {
        let s = self.call(obj, "toString", "()Ljava/lang/String;", &[])?;
        self.rust_string(&s)
    }

    fn f64(&mut self, boxed: &JObject<'a>) -> Result<f64> {
        let r = self
            .env
            .call_method(boxed, "doubleValue", "()D", &[])
            .and_then(|v| v.d());
        self.check(r, "doubleValue")
    }

    fn i64(&mut self, boxed: &JObject<'a>) -> Result<i64> {
        let r = self
            .env
            .call_method(boxed, "longValue", "()J", &[])
            .and_then(|v| v.j());
        self.check(r, "longValue")
    }

    fn bool(&mut self, boxed: &JObject<'a>) -> Result<bool> {
        let r = self
            .env
            .call_method(boxed, "booleanValue", "()Z", &[])
            .and_then(|v| v.z());
        self.check(r, "booleanValue")
    }

    fn class(&mut self, name: &str) -> Result<JObject<'a>> {
        let r = self.env.find_class(name);
        Ok(JObject::from(self.check(r, name)?))
    }

    fn class_name(&mut self, class: &JObject<'a>) -> Result<String> {
        let n = self.call(class, "getName", "()Ljava/lang/String;", &[])?;
        self.rust_string(&n)
    }

    fn class_of(&mut self, obj: &JObject<'a>) -> Result<JObject<'a>> {
        self.call(obj, "getClass", "()Ljava/lang/Class;", &[])
    }

    fn array(&mut self, obj: JObject<'a>) -> Result<Vec<JObject<'a>>> {
        let arr = JObjectArray::from(obj);
        let n = self.env.get_array_length(&arr)?;
        let mut out = Vec::new();
        for i in 0..n {
            let r = self.env.get_object_array_element(&arr, i);
            out.push(self.check(r, "array element")?);
        }
        Ok(out)
    }

    /// A public static field's value: `Class.getField(name).get(null)`.
    fn static_field(&mut self, class: &str, field: &str) -> Result<JObject<'a>> {
        let cls = self.class(class)?;
        let name = self.string(field)?;
        let f = self.call(
            &cls,
            "getField",
            "(Ljava/lang/String;)Ljava/lang/reflect/Field;",
            &[JValue::Object(&name)],
        )?;
        self.call(
            &f,
            "get",
            "(Ljava/lang/Object;)Ljava/lang/Object;",
            &[JValue::Object(&JObject::null())],
        )
        .with_context(|| format!("{class}.{field}"))
    }

    /// Do the method's parameter types accept these arguments (a primitive accepts its box)?
    fn accepts(&mut self, method: &JObject<'a>, args: &[&JObject<'a>]) -> Result<bool> {
        let types = self.call(method, "getParameterTypes", "()[Ljava/lang/Class;", &[])?;
        let types = self.array(types)?;
        for (t, a) in types.iter().zip(args) {
            let r = self
                .env
                .call_method(t, "isPrimitive", "()Z", &[])
                .and_then(|v| v.z());
            if self.check(r, "isPrimitive")? {
                continue;
            }
            let r = self
                .env
                .call_method(
                    t,
                    "isInstance",
                    "(Ljava/lang/Object;)Z",
                    &[JValue::Object(a)],
                )
                .and_then(|v| v.z());
            if !self.check(r, "isInstance")? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// The public method of `class` with this name that takes these arguments.
    fn method(
        &mut self,
        class: &JObject<'a>,
        name: &str,
        args: &[&JObject<'a>],
    ) -> Result<GlobalRef> {
        let class_name = self.class_name(class)?;
        let key = (class_name.clone(), name.to_owned(), args.len());
        if let Some(m) = self.methods.get(&key) {
            return Ok(m.clone());
        }
        let all = self.call(class, "getMethods", "()[Ljava/lang/reflect/Method;", &[])?;
        let mut found = None;
        for m in self.array(all)? {
            let n = self.call(&m, "getName", "()Ljava/lang/String;", &[])?;
            if self.rust_string(&n)? != name {
                continue;
            }
            let r = self
                .env
                .call_method(&m, "getParameterCount", "()I", &[])
                .and_then(|v| v.i());
            if usize::try_from(self.check(r, "getParameterCount")?)? != args.len() {
                continue;
            }
            if self.accepts(&m, args)? {
                found = Some(m);
                break;
            }
        }
        let m = found.with_context(|| {
            format!(
                "no method {class_name}.{name} taking {} arguments",
                args.len()
            )
        })?;
        // A public method of a non-public class (an anonymous registry iterator, MappedRegistry$3)
        // cannot be invoked from a JNI-attached thread unless it is made accessible.
        let r = self
            .env
            .call_method(&m, "setAccessible", "(Z)V", &[JValue::Bool(1)]);
        self.check(r, "setAccessible")?;
        let global = self.env.new_global_ref(m)?;
        self.methods.insert(key, global.clone());
        Ok(global)
    }

    fn object_array(&mut self, args: &[&JObject<'a>]) -> Result<JObjectArray<'a>> {
        let len = i32::try_from(args.len())?;
        let r = self
            .env
            .new_object_array(len, "java/lang/Object", JObject::null());
        let arr = self.check(r, "new Object[]")?;
        for (i, a) in args.iter().enumerate() {
            self.env
                .set_object_array_element(&arr, i32::try_from(i)?, a)?;
        }
        Ok(arr)
    }

    fn invoke_method(
        &mut self,
        method: &GlobalRef,
        target: &JObject<'a>,
        args: &[&JObject<'a>],
        what: &str,
    ) -> Result<JObject<'a>> {
        let arr = self.object_array(args)?;
        let r = self
            .env
            .call_method(
                method,
                "invoke",
                "(Ljava/lang/Object;[Ljava/lang/Object;)Ljava/lang/Object;",
                &[JValue::Object(target), JValue::Object(&arr)],
            )
            .and_then(|v| v.l());
        self.check(r, what)
    }

    /// `obj.name(args...)`, the method found by name and arguments on the object's class.
    fn invoke(
        &mut self,
        obj: &JObject<'a>,
        name: &str,
        args: &[&JObject<'a>],
    ) -> Result<JObject<'a>> {
        let class = self.class_of(obj)?;
        let m = self.method(&class, name, args)?;
        self.invoke_method(&m, obj, args, name)
    }

    /// `Class.name(args...)` for a static method.
    fn invoke_static(
        &mut self,
        class: &str,
        name: &str,
        args: &[&JObject<'a>],
    ) -> Result<JObject<'a>> {
        let cls = self.class(class)?;
        let m = self.method(&cls, name, args)?;
        self.invoke_method(&m, &JObject::null(), args, &format!("{class}.{name}"))
    }

    /// Every element of a `java.lang.Iterable`.
    fn iterate(&mut self, iterable: &JObject<'a>) -> Result<Vec<JObject<'a>>> {
        let it = self.call(iterable, "iterator", "()Ljava/util/Iterator;", &[])?;
        let mut out = Vec::new();
        loop {
            let r = self
                .env
                .call_method(&it, "hasNext", "()Z", &[])
                .and_then(|v| v.z());
            if !self.check(r, "hasNext")? {
                return Ok(out);
            }
            out.push(self.call(&it, "next", "()Ljava/lang/Object;", &[])?);
        }
    }

    /// A registry entry's name without its namespace: `registry.getKey(entry).toString()`.
    fn key(&mut self, registry: &JObject<'a>, entry: &JObject<'a>) -> Result<String> {
        let k = self.invoke(registry, "getKey", &[entry])?;
        Ok(strip_ns(&self.display(&k)?).to_owned())
    }

    /// Declared fields of a class: `(name, field, is static)`.
    fn declared_fields(&mut self, class: &JObject<'a>) -> Result<Vec<(String, JObject<'a>, bool)>> {
        let fields = self.call(
            class,
            "getDeclaredFields",
            "()[Ljava/lang/reflect/Field;",
            &[],
        )?;
        let mut out = Vec::new();
        for f in self.array(fields)? {
            let n = self.call(&f, "getName", "()Ljava/lang/String;", &[])?;
            let name = self.rust_string(&n)?;
            let r = self
                .env
                .call_method(&f, "getModifiers", "()I", &[])
                .and_then(|v| v.i());
            let is_static = self.check(r, "getModifiers")? & 0x8 != 0;
            out.push((name, f, is_static));
        }
        Ok(out)
    }

    fn field_get(&mut self, field: &JObject<'a>, target: &JObject<'a>) -> Result<JObject<'a>> {
        self.call(
            field,
            "get",
            "(Ljava/lang/Object;)Ljava/lang/Object;",
            &[JValue::Object(target)],
        )
    }

    fn is_instance(&mut self, class: &JObject<'a>, obj: &JObject<'a>) -> Result<bool> {
        let r = self
            .env
            .call_method(
                class,
                "isInstance",
                "(Ljava/lang/Object;)Z",
                &[JValue::Object(obj)],
            )
            .and_then(|v| v.z());
        self.check(r, "isInstance")
    }
}

const REGISTRIES: &str = "net/minecraft/core/registries/BuiltInRegistries";

fn probe_blocks<'a>(r: &mut Reflect<'a, '_>) -> Result<BTreeMap<String, BlockProbe>> {
    let reg = r.static_field(REGISTRIES, "BLOCK")?;
    let getter = r.static_field("net/minecraft/world/level/EmptyBlockGetter", "INSTANCE")?;
    let zero = r.static_field("net/minecraft/core/BlockPos", "ZERO")?;
    let axis_y = r.static_field("net/minecraft/core/Direction$Axis", "Y")?;
    let full = r.invoke_static("net/minecraft/world/phys/shapes/Shapes", "block", &[])?;
    let mut out = BTreeMap::new();
    for block in r.iterate(&reg)? {
        let name = r.key(&reg, &block)?;
        let state = r.invoke(&block, "defaultBlockState", &[])?;
        let h = r.invoke(&state, "getDestroySpeed", &[&getter, &zero])?;
        let hardness = r.f64(&h)?;
        let n = r.invoke(&state, "requiresCorrectToolForDrops", &[])?;
        let needs_correct_tool = r.bool(&n)?;
        let shape = r.invoke(&state, "getCollisionShape", &[&getter, &zero])?;
        let e = r.invoke(&shape, "isEmpty", &[])?;
        let collision = if r.bool(&e)? {
            Collision::Empty
        } else {
            let res = r
                .env
                .call_method(
                    &shape,
                    "equals",
                    "(Ljava/lang/Object;)Z",
                    &[JValue::Object(&full)],
                )
                .and_then(|v| v.z());
            if r.check(res, "equals")? {
                Collision::Full
            } else {
                let top = r.invoke(&shape, "max", &[&axis_y])?;
                Collision::Partial(r.f64(&top)?)
            }
        };
        out.insert(
            name,
            BlockProbe {
                hardness,
                needs_correct_tool,
                collision,
            },
        );
    }
    Ok(out)
}

fn probe_materials<'a>(r: &mut Reflect<'a, '_>) -> Result<BTreeMap<String, Material>> {
    let cls = r.class("net/minecraft/world/item/ToolMaterial")?;
    let mut out = BTreeMap::new();
    for (name, field, is_static) in r.declared_fields(&cls)? {
        if !is_static {
            continue;
        }
        let m = r.field_get(&field, &JObject::null())?;
        if m.is_null() || !r.is_instance(&cls, &m)? {
            continue;
        }
        let s = r.invoke(&m, "speed", &[])?;
        let d = r.invoke(&m, "durability", &[])?;
        let tag = r.invoke(&m, "incorrectBlocksForDrops", &[])?;
        let loc = r.invoke(&tag, "location", &[])?;
        out.insert(
            name.to_lowercase(),
            Material {
                speed: r.f64(&s)?,
                durability: r.i64(&d)?,
                incorrect: strip_ns(&r.display(&loc)?).to_owned(),
            },
        );
    }
    Ok(out)
}

/// Fuel is given partly by item tag (`#logs`), so the vanilla data pack's tags are loaded and
/// bound first, as clojurecraft's `fuel` does; the burn times are `FuelValues`' own item map.
fn probe_fuel<'a>(r: &mut Reflect<'a, '_>) -> Result<BTreeMap<String, i64>> {
    let pack_type = r.static_field("net/minecraft/server/packs/PackType", "SERVER_DATA")?;
    let pack = r.invoke_static(
        "net/minecraft/server/packs/repository/ServerPacksSource",
        "createVanillaPackSource",
        &[],
    )?;
    let res = r
        .env
        .call_static_method(
            "java/util/Collections",
            "singletonList",
            "(Ljava/lang/Object;)Ljava/util/List;",
            &[JValue::Object(&pack)],
        )
        .and_then(|v| v.l());
    let packs = r.check(res, "singletonList")?;
    let rm_class = r.class("net/minecraft/server/packs/resources/MultiPackResourceManager")?;
    let ctors = r.call(
        &rm_class,
        "getConstructors",
        "()[Ljava/lang/reflect/Constructor;",
        &[],
    )?;
    let mut rm = None;
    for c in r.array(ctors)? {
        let res = r
            .env
            .call_method(&c, "getParameterCount", "()I", &[])
            .and_then(|v| v.i());
        if r.check(res, "getParameterCount")? == 2 {
            let args = r.object_array(&[&pack_type, &packs])?;
            let res = r
                .env
                .call_method(
                    &c,
                    "newInstance",
                    "([Ljava/lang/Object;)Ljava/lang/Object;",
                    &[JValue::Object(&args)],
                )
                .and_then(|v| v.l());
            rm = Some(r.check(res, "new MultiPackResourceManager")?);
            break;
        }
    }
    let rm = rm.context("no two-argument MultiPackResourceManager constructor")?;
    let registries = r.static_field(REGISTRIES, "REGISTRY")?;
    let access = r.invoke_static(
        "net/minecraft/core/RegistryAccess",
        "fromRegistryOfRegistries",
        &[&registries],
    )?;
    let pending = r.invoke_static(
        "net/minecraft/tags/TagLoader",
        "loadTagsForExistingRegistries",
        &[&rm, &access],
    )?;
    for p in r.iterate(&pending)? {
        r.invoke(&p, "apply", &[])?;
    }
    let flags = r.static_field("net/minecraft/world/flag/FeatureFlags", "VANILLA_SET")?;
    let fv = r.invoke_static(
        "net/minecraft/world/level/block/entity/FuelValues",
        "vanillaBurnTimes",
        &[&access, &flags],
    )?;
    let fv_class = r.class("net/minecraft/world/level/block/entity/FuelValues")?;
    let name = r.string("values")?;
    let field = r.call(
        &fv_class,
        "getDeclaredField",
        "(Ljava/lang/String;)Ljava/lang/reflect/Field;",
        &[JValue::Object(&name)],
    )?;
    let res = r
        .env
        .call_method(&field, "setAccessible", "(Z)V", &[JValue::Bool(1)]);
    r.check(res, "setAccessible")?;
    let values = r.field_get(&field, &fv)?;
    let items = r.static_field(REGISTRIES, "ITEM")?;
    let entries = r.call(&values, "entrySet", "()Ljava/util/Set;", &[])?;
    let mut out = BTreeMap::new();
    for e in r.iterate(&entries)? {
        let item = r.call(&e, "getKey", "()Ljava/lang/Object;", &[])?;
        let ticks = r.call(&e, "getValue", "()Ljava/lang/Object;", &[])?;
        let name = r.key(&items, &item)?;
        out.insert(name, r.i64(&ticks)?);
    }
    Ok(out)
}

fn probe_mobs<'a>(r: &mut Reflect<'a, '_>) -> Result<BTreeMap<String, Mob>> {
    const ATTRS: &str = "net/minecraft/world/entity/ai/attributes/DefaultAttributes";
    let reg = r.static_field(REGISTRIES, "ENTITY_TYPE")?;
    let max_health = r.static_field(
        "net/minecraft/world/entity/ai/attributes/Attributes",
        "MAX_HEALTH",
    )?;
    let mut out = BTreeMap::new();
    for t in r.iterate(&reg)? {
        let has = r.invoke_static(ATTRS, "hasSupplier", &[&t])?;
        if !r.bool(&has)? {
            continue;
        }
        let name = r.key(&reg, &t)?;
        let supplier = r.invoke_static(ATTRS, "getSupplier", &[&t])?;
        let h = r.invoke(&supplier, "getValue", &[&max_health])?;
        let cat = r.invoke(&t, "getCategory", &[])?;
        let cat_name = r.invoke(&cat, "getName", &[])?;
        out.insert(
            name,
            Mob {
                max_health: r.f64(&h)?,
                category: r.rust_string(&cat_name)?,
            },
        );
    }
    Ok(out)
}

/// The serializer constants of `EntityDataSerializers` and their ids: the index an entity-data
/// entry names its value type by on the wire. The order is load-bearing (brain/wire/wire-entity-metadata.md).
fn probe_serializers<'a>(r: &mut Reflect<'a, '_>) -> Result<BTreeMap<String, i64>> {
    const SERIALIZERS: &str = "net/minecraft/network/syncher/EntityDataSerializers";
    let cls = r.class(SERIALIZERS)?;
    let iface = r.class("net/minecraft/network/syncher/EntityDataSerializer")?;
    let mut out = BTreeMap::new();
    for (name, field, is_static) in r.declared_fields(&cls)? {
        if !is_static {
            continue;
        }
        let res = r
            .env
            .call_method(&field, "setAccessible", "(Z)V", &[JValue::Bool(1)]);
        r.check(res, "setAccessible")?;
        let v = r.field_get(&field, &JObject::null())?;
        if v.is_null() || !r.is_instance(&iface, &v)? {
            continue;
        }
        let id = r.invoke_static(SERIALIZERS, "getSerializedId", &[&v])?;
        out.insert(name, r.i64(&id)?);
    }
    let ids: BTreeSet<i64> = out.values().copied().collect();
    ensure!(ids.len() == out.len(), "two serializers share an id");
    ensure!(
        ids.iter().copied().eq(0..i64::try_from(ids.len())?),
        "serializer ids do not run 0..{}",
        ids.len()
    );
    Ok(out)
}

/// Start the JVM on the bundler's extracted inner jar and libraries (written into `work` when the
/// reports ran), boot the game and read every table.
pub fn run(work: &Path) -> Result<Probe> {
    let inner = work
        .join("versions")
        .join(VERSION)
        .join(format!("server-{VERSION}.jar"));
    ensure!(
        inner.exists(),
        "{} is missing: the bundler extracts it when the reports run",
        inner.display()
    );
    let mut jars = vec![inner];
    jars_under(&work.join("libraries"), &mut jars)?;
    let class_path = jars
        .iter()
        .map(|p| p.display().to_string())
        .collect::<Vec<_>>()
        .join(":");
    let args = InitArgsBuilder::new()
        .version(JNIVersion::V8)
        .option(format!("-Djava.class.path={class_path}"))
        .option("-Xmx3G")
        .option("-Djava.awt.headless=true")
        .build()
        .map_err(|e| anyhow!("JVM arguments: {e}"))?;
    let jvm = JavaVM::new(args)
        .map_err(|e| anyhow!("start the JVM (JAVA_HOME must name Java 25): {e}"))?;
    let mut guard = jvm.attach_current_thread()?;
    let mut r = Reflect {
        env: &mut guard,
        methods: BTreeMap::new(),
    };

    r.invoke_static("net/minecraft/SharedConstants", "tryDetectVersion", &[])?;
    r.invoke_static("net/minecraft/server/Bootstrap", "bootStrap", &[])?;
    println!("probe: game bootstrapped");

    let probe = Probe {
        blocks: probe_blocks(&mut r).context("blocks")?,
        materials: probe_materials(&mut r).context("tool materials")?,
        fuel: probe_fuel(&mut r).context("fuel")?,
        mobs: probe_mobs(&mut r).context("mobs")?,
        serializers: probe_serializers(&mut r).context("entity data serializers")?,
    };
    println!(
        "probe: {} blocks, {} materials, {} fuels, {} mobs, {} serializers",
        probe.blocks.len(),
        probe.materials.len(),
        probe.fuel.len(),
        probe.mobs.len(),
        probe.serializers.len()
    );
    if probe.blocks.is_empty() {
        bail!("the probe read no blocks");
    }
    Ok(probe)
}
