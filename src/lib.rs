use articy::{
    types::{File as ArticyFile, Id, Model},
    Interpreter as ArticyInterpreter, Outcome, StateValue,
};
use gdnative::api::PackedDataContainer;
use gdnative::prelude::*;
use std::rc::Rc;

#[derive(NativeClass, Debug, Default)]
#[inherit(Node)]
#[register_with(Self::register_signals)]
pub struct Database {
    #[property]
    articy_resource: Option<Ref<PackedDataContainer>>,
    pub file: Option<Rc<ArticyFile>>,
}

#[derive(ToVariant, Debug)]
pub struct Dialogue {
    id: String,
    name: String,
}

#[derive(Debug)]
pub enum Error {
    DatabaseNotSetup,
    InterpreterNotSetup,
    FailedToSetState,
    FailedToGetState,
    ArticyError(articy::types::Error),
}

#[methods]
impl Database {
    fn new(_base: &Node) -> Self {
        Default::default()
    }

    fn register_signals(builder: &ClassBuilder<Self>) {
        builder.signal("loaded").done();
    }

    #[method]
    fn _ready(&mut self, #[base] owner: &Node) {
        if let Some(resource) = &self.articy_resource {
            self.load(owner, resource.clone());
        } else if let Some(node) = owner.get_parent() {
            let name = unsafe {
                match node.assume_safe().get("name").dispatch() {
                    VariantDispatch::GodotString(godot_string) => godot_string.to_string(),
                    _ => "".to_owned(),
                }
            };

            if name == "root" {
                godot_print!("Detected as AutoLoad, attempting to read project setting \"articy/autoload_database_path\" to load resource from");
                let settings = gdnative::api::ProjectSettings::godot_singleton();

                if settings.has_setting("articy/autoload_database_path") {
                    let path = settings
                        .get_setting("articy/autoload_database_path")
                        .to_string();

                    let resource = load::<gdnative::api::PackedDataContainer>(path).expect("the resource to be loaded from \"articy/autoload_database_path\" to be of type `PackedDataContainer` (as imported by the plugin).");

                    self.load(owner, resource);
                } else {
                    godot_error!(
                        "Your project does not have \"articy/autoload_database_path\" set."
                    )
                }
            }
        }
    }

    #[method]
    fn load(
        &mut self,
        #[base] owner: &Node,
        resource: Ref<gdnative::api::PackedDataContainer, Shared>,
    ) {
        let resource = unsafe { resource.assume_safe() };

        let bytes = resource
            .get("__data__")
            .to::<PoolArray<u8>>()
            .expect("__data__ to be of type PoolArray<u8> (PoolByteArray)");

        let file: ArticyFile =
            serde_json::from_slice(&bytes.to_vec()).expect("to be able to parse articy data");

        self.file = Some(Rc::from(file));

        owner.emit_signal("loaded", &[]);
    }

    #[method]
    fn get_model(&self, id: String) -> Option<ArticyModel<'_>> {
        self.file
            .as_ref()?
            .get_default_package()
            .models
            .iter()
            .find_map(|model| {
                if model.id().to_inner() == id {
                    Some(ArticyModel(model))
                } else {
                    None
                }
            })
    }

    #[method]
    fn get_models_of_type(&self, kind: String) -> Vec<ArticyModel<'_>> {
        self.file
            .as_ref()
            .ok_or(Error::DatabaseNotSetup)
            .unwrap()
            .get_models_of_type(&kind)
            .iter()
            .map(|model| ArticyModel(model))
            .collect::<Vec<ArticyModel<'_>>>()
    }
}

#[derive(NativeClass, Default)]
#[inherit(Node)]
#[register_with(Self::register_signals)]
struct Interpreter {
    #[property]
    database_path: Option<NodePath>,
    interpreter: Option<ArticyInterpreter>,
}

#[methods]
impl Interpreter {
    fn new(_base: &Node) -> Self {
        Default::default()
    }

    fn register_signals(builder: &ClassBuilder<Self>) {
        builder.signal("started").done();

        builder
            .signal("line")
            .with_param("line", VariantType::Dictionary)
            .done();

        builder
            .signal("choices")
            .with_param("choices", VariantType::VariantArray)
            .done();

        builder
            .signal("model")
            .with_param("model", VariantType::Dictionary)
            .done();

        builder.signal("stopped").done();
    }

    #[method]
    fn _ready(&mut self, #[base] owner: &Node) {
        if let Some(path) = &self.database_path {
            self.set_database(owner, path.new_ref())
        }
    }

    #[method]
    // TODO: Perhaps do a getter and a setter on the node_path exported property instead of a method
    fn set_database(&mut self, #[base] owner: &Node, path: NodePath) {
        let node = owner
            .get_node(path.to_godot_string())
            .expect("To find node for path");

        let file = unsafe {
            node.assume_safe()
                .cast_instance::<Database>()
                .expect("to find a Database type from the Articy integration")
                .map(|data, _base| data.file.clone())
                .expect("to get `file` mapped from the Articy Database Rust type")
        }
        .expect("for the Articy Database to have a file loaded");

        // NOTE: You can also just add the Database in your scene instead of as an AutoLoad, and refer to it with $Database
        self.interpreter = Some(ArticyInterpreter::new(file));

        godot_print!("Loaded Articy Interpreter with \"{path:?}\" as a source!");
    }

    #[method]
    fn set_state(&mut self, key: GodotString, value: Variant) {
        let interpreter = self
            .interpreter
            .as_mut()
            .ok_or(Error::InterpreterNotSetup)
            .unwrap();

        interpreter.set_state(
            &key.to_string(),
            match value.dispatch() {
                VariantDispatch::Nil => StateValue::Empty,
                VariantDispatch::Bool(bool) => StateValue::Boolean(bool),
                VariantDispatch::I64(integer) => StateValue::Int(integer),
                VariantDispatch::F64(float) => StateValue::Float(float),
                VariantDispatch::GodotString(string) => StateValue::String(string.to_string()),
                VariantDispatch::NodePath(path) => StateValue::String(path.to_string()),

                VariantDispatch::Vector2(..)
                | VariantDispatch::Vector3(..)
                | VariantDispatch::Quat(..)
                | VariantDispatch::Transform2D(..)
                | VariantDispatch::Plane(..)
                | VariantDispatch::Aabb(..)
                | VariantDispatch::Basis(..)
                | VariantDispatch::Transform(..)
                | VariantDispatch::Color(..)
                | VariantDispatch::Rid(..)
                | VariantDispatch::Object(..)
                | VariantDispatch::Dictionary(..) // TODO: Might wanna serialize this to a string and store as such?
                | VariantDispatch::VariantArray(..) // TODO: Find a usecase for arrays / tuples
                | VariantDispatch::ByteArray(..)
                | VariantDispatch::Int32Array(..)
                | VariantDispatch::Float32Array(..)
                | VariantDispatch::StringArray(..)
                | VariantDispatch::Vector2Array(..)
                | VariantDispatch::Vector3Array(..)
                | VariantDispatch::ColorArray(..)
                | VariantDispatch::Rect2(..) => panic!("Type not supported for serialisation in Articy"),
            },
        )
        .ok()
        .ok_or(Error::FailedToSetState)
        .unwrap()
    }

    #[method]
    fn get_state(&mut self, key: GodotString) -> Variant {
        let interpreter = self
            .interpreter
            .as_mut()
            .ok_or(Error::InterpreterNotSetup)
            .unwrap();

        match interpreter
            .get_state(&key.to_string())
            .ok()
            .ok_or(Error::FailedToGetState)
            .unwrap()
        {
            StateValue::String(string) => Variant::new(GodotString::from_str(string)),
            StateValue::Float(float) => Variant::new(float),
            StateValue::Int(int) => Variant::new(int),
            StateValue::Boolean(bool) => Variant::new(bool),
            StateValue::Empty => Variant::nil(),
            StateValue::Tuple(..) => {
                unimplemented!("did not implement recursion to deserialize arrays")
            }
        }
    }

    #[method]
    fn start(&mut self, #[base] owner: &Node, id: String) {
        let interpreter = self
            .interpreter
            .as_mut()
            .ok_or(Error::InterpreterNotSetup)
            .unwrap();

        interpreter
            .start(Id(id))
            .map_err(Error::ArticyError)
            .unwrap();

        let model = interpreter
            .get_current_model()
            .map_err(Error::ArticyError)
            .unwrap();

        match model {
            Model::DialogueFragment {
                text,
                id,
                speaker,
                technical_name,
                ..
            } => {
                let dictionary = Dictionary::new();
                dictionary.insert("line", text.to_owned());
                dictionary.insert("id", id.to_inner());
                dictionary.insert("speaker", speaker.to_inner());
                dictionary.insert("technical_name", technical_name.to_owned());

                owner.emit_signal("line", &[Variant::new(dictionary)]);
            }
            model => {
                owner.emit_signal("model", &[ArticyModel(model).to_variant()]);
            }
        }
    }

    #[method]
    fn advance(&mut self, #[base] owner: &Node) {
        let interpreter = self
            .interpreter
            .as_mut()
            .ok_or(Error::InterpreterNotSetup)
            .unwrap();

        match interpreter.advance() {
            Ok(outcome) => match outcome {
                Outcome::Advanced(Model::DialogueFragment {
                    id,
                    text,
                    speaker,
                    technical_name,
                    ..
                }) => {
                    let dictionary = Dictionary::new();

                    dictionary.insert("id", id.to_inner());
                    dictionary.insert("line", text.to_owned());
                    dictionary.insert("speaker", speaker.to_inner());
                    dictionary.insert("technical_name", technical_name.to_owned());

                    owner.emit_signal("line", &[Variant::new(dictionary)]);
                }
                Outcome::Advanced(other_model) => {
                    owner.emit_signal("model", &[ArticyModel(other_model).to_variant()]);
                }
                Outcome::WaitingForChoice(choices) => {
                    let array = VariantArray::new();
                    for choice in choices {
                        let dictionary = Dictionary::new();
                        match choice {
                            Model::DialogueFragment { text, id, .. } => {
                                dictionary.insert("label", text.to_owned());
                                dictionary.insert("id", id.to_inner());

                                array.push(dictionary);
                            }
                            other_model => {
                                owner
                                    .emit_signal("model", &[ArticyModel(other_model).to_variant()]);
                            }
                        }
                    }

                    owner.emit_signal("choices", &[Variant::new(array)]);
                }
                Outcome::Stopped | Outcome::EndOfDialogue => {
                    owner.emit_signal("stopped", &[]);
                }
            },
            Err(error) => godot_error!("Interpreter.advance() returned an error: {error:#?}"),
        }
    }

    #[method]
    fn choose(&mut self, id: String) {
        let interpreter = self
            .interpreter
            .as_mut()
            .ok_or(Error::InterpreterNotSetup)
            .unwrap();

        interpreter
            .choose(Id(id))
            .map_err(Error::ArticyError)
            .unwrap();
    }
}

struct ArticyModel<'a>(&'a Model);

impl ToVariant for ArticyModel<'_> {
    // TODO: Replace with manual deserialisation, current implementation can't rename properties consistently
    // TODO: Maybe replace Type / Properties with a flat "Properties" dictionary with a "type" key
    fn to_variant(&self) -> Variant {
        match self.0 {
            Model::Custom(kind, value) => {
                let json =
                    serde_json::to_string(&serde_json::json!({"Type": kind, "Properties": value}))
                        .expect("articy-rs to produce proper JSON");

                unsafe {
                    gdnative::api::JSON::godot_singleton()
                        .parse(json)
                        .expect("articy-rs JSON to be parseable by Godot")
                        .assume_safe()
                        .result()
                }
            }
            _ => {
                let json = serde_json::to_string(self.0).expect("articy-rs to produce proper JSON");

                unsafe {
                    gdnative::api::JSON::godot_singleton()
                        .parse(json)
                        .expect("articy-rs JSON to be parseable by Godot")
                        .assume_safe()
                        .result()
                }
            }
        }
    }
}

fn init(handle: InitHandle) {
    handle.add_class::<Database>();
    handle.add_class::<Interpreter>();
}

godot_init!(init);
