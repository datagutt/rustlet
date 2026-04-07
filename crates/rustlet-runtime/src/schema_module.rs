use starlark::environment::GlobalsBuilder;
use starlark::eval::Evaluator;
use starlark::values::none::NoneType;
use starlark::values::structs::AllocStruct;
use starlark::values::Value;

// Schema module provides configuration metadata for app stores.
// These constructors return inert structs. They don't affect rendering.

#[starlark::starlark_module]
pub fn schema_module(builder: &mut GlobalsBuilder) {
    fn Schema<'v>(
        #[starlark(default = "1")] version: &str,
        #[starlark(default = NoneType)] fields: Value<'v>,
        #[starlark(default = NoneType)] handlers: Value<'v>,
        #[starlark(default = NoneType)] notifications: Value<'v>,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let heap = eval.heap();
        Ok(heap.alloc(AllocStruct([
            ("version", heap.alloc(version)),
            ("fields", fields),
            ("handlers", handlers),
            ("notifications", notifications),
        ])))
    }

    fn Toggle<'v>(
        id: &str,
        name: &str,
        desc: &str,
        icon: &str,
        #[starlark(default = false)] default: bool,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let heap = eval.heap();
        Ok(heap.alloc(AllocStruct([
            ("id", heap.alloc(id)),
            ("name", heap.alloc(name)),
            ("desc", heap.alloc(desc)),
            ("icon", heap.alloc(icon)),
            ("default", heap.alloc(default)),
        ])))
    }

    fn Text<'v>(
        id: &str,
        name: &str,
        desc: &str,
        icon: &str,
        #[starlark(default = "")] default: &str,
        #[starlark(default = false)] secret: bool,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let heap = eval.heap();
        Ok(heap.alloc(AllocStruct([
            ("id", heap.alloc(id)),
            ("name", heap.alloc(name)),
            ("desc", heap.alloc(desc)),
            ("icon", heap.alloc(icon)),
            ("default", heap.alloc(default)),
            ("secret", heap.alloc(secret)),
        ])))
    }

    fn Option<'v>(
        display: &str,
        value: &str,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let heap = eval.heap();
        Ok(heap.alloc(AllocStruct([
            ("display", heap.alloc(display)),
            ("value", heap.alloc(value)),
        ])))
    }

    fn Dropdown<'v>(
        id: &str,
        name: &str,
        desc: &str,
        icon: &str,
        default: &str,
        options: Value<'v>,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let heap = eval.heap();
        Ok(heap.alloc(AllocStruct([
            ("id", heap.alloc(id)),
            ("name", heap.alloc(name)),
            ("desc", heap.alloc(desc)),
            ("icon", heap.alloc(icon)),
            ("default", heap.alloc(default)),
            ("options", options),
        ])))
    }

    fn Location<'v>(
        id: &str,
        name: &str,
        desc: &str,
        icon: &str,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let heap = eval.heap();
        Ok(heap.alloc(AllocStruct([
            ("id", heap.alloc(id)),
            ("name", heap.alloc(name)),
            ("desc", heap.alloc(desc)),
            ("icon", heap.alloc(icon)),
        ])))
    }

    fn LocationBased<'v>(
        id: &str,
        name: &str,
        desc: &str,
        icon: &str,
        handler: Value<'v>,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let heap = eval.heap();
        Ok(heap.alloc(AllocStruct([
            ("id", heap.alloc(id)),
            ("name", heap.alloc(name)),
            ("desc", heap.alloc(desc)),
            ("icon", heap.alloc(icon)),
            ("handler", handler),
        ])))
    }

    fn DateTime<'v>(
        id: &str,
        name: &str,
        desc: &str,
        icon: &str,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let heap = eval.heap();
        Ok(heap.alloc(AllocStruct([
            ("id", heap.alloc(id)),
            ("name", heap.alloc(name)),
            ("desc", heap.alloc(desc)),
            ("icon", heap.alloc(icon)),
        ])))
    }

    fn OAuth2<'v>(
        id: &str,
        name: &str,
        desc: &str,
        icon: &str,
        handler: Value<'v>,
        client_id: &str,
        authorization_endpoint: &str,
        #[starlark(default = NoneType)] scopes: Value<'v>,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let heap = eval.heap();
        Ok(heap.alloc(AllocStruct([
            ("id", heap.alloc(id)),
            ("name", heap.alloc(name)),
            ("desc", heap.alloc(desc)),
            ("icon", heap.alloc(icon)),
            ("handler", handler),
            ("client_id", heap.alloc(client_id)),
            ("authorization_endpoint", heap.alloc(authorization_endpoint)),
            ("scopes", scopes),
        ])))
    }

    fn PhotoSelect<'v>(
        id: &str,
        name: &str,
        desc: &str,
        icon: &str,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let heap = eval.heap();
        Ok(heap.alloc(AllocStruct([
            ("id", heap.alloc(id)),
            ("name", heap.alloc(name)),
            ("desc", heap.alloc(desc)),
            ("icon", heap.alloc(icon)),
        ])))
    }

    fn Typeahead<'v>(
        id: &str,
        name: &str,
        desc: &str,
        icon: &str,
        handler: Value<'v>,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let heap = eval.heap();
        Ok(heap.alloc(AllocStruct([
            ("id", heap.alloc(id)),
            ("name", heap.alloc(name)),
            ("desc", heap.alloc(desc)),
            ("icon", heap.alloc(icon)),
            ("handler", handler),
        ])))
    }

    fn Color<'v>(
        id: &str,
        name: &str,
        desc: &str,
        icon: &str,
        default: &str,
        #[starlark(default = NoneType)] palette: Value<'v>,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let heap = eval.heap();
        Ok(heap.alloc(AllocStruct([
            ("id", heap.alloc(id)),
            ("name", heap.alloc(name)),
            ("desc", heap.alloc(desc)),
            ("icon", heap.alloc(icon)),
            ("default", heap.alloc(default)),
            ("palette", palette),
        ])))
    }

    fn Generated<'v>(
        source: &str,
        handler: Value<'v>,
        id: &str,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let heap = eval.heap();
        Ok(heap.alloc(AllocStruct([
            ("source", heap.alloc(source)),
            ("handler", handler),
            ("id", heap.alloc(id)),
        ])))
    }

    fn Notification<'v>(
        id: &str,
        name: &str,
        desc: &str,
        icon: &str,
        #[starlark(default = NoneType)] sounds: Value<'v>,
        #[starlark(default = NoneType)] builder: Value<'v>,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let heap = eval.heap();
        Ok(heap.alloc(AllocStruct([
            ("id", heap.alloc(id)),
            ("name", heap.alloc(name)),
            ("desc", heap.alloc(desc)),
            ("icon", heap.alloc(icon)),
            ("sounds", sounds),
            ("builder", builder),
        ])))
    }
}

pub fn build_schema_globals() -> starlark::environment::Globals {
    starlark::environment::GlobalsBuilder::new()
        .with(schema_module)
        .build()
}
