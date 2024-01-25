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

        self.file = Some(Rc::from(ArticyFile::from_buffer(&bytes.to_vec())));
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

    #[method]
    fn get_model_by_external_id(&self, external_id: String) -> Option<ArticyModel<'_>> {
        self.file
            .as_ref()?
            .get_default_package()
            .models
            .iter()
            .find_map(|model| {
                if model.external_id().to_inner() == external_id {
                    Some(ArticyModel(model))
                } else {
                    None
                }
            })
    }

    #[method]
    fn get_all_models(&self) -> Vec<ArticyModel<'_>> {
        self.file
            .as_ref()
            .ok_or(Error::DatabaseNotSetup)
            .unwrap()
            .get_models()
            .iter()
            .map(|model| ArticyModel(model))
            .collect::<Vec<ArticyModel<'_>>>()
    }

    #[method]
    fn get_entity_ids_from_folder(&self, folder_id: String) -> Vec<String> {
        let model = self
            .get_model(folder_id.clone())
            .expect("to find model for folder_id")
            .0;
        if let Model::UserFolder { .. } = model {
            model
        } else {
            panic!("{folder_id:?} isn't a UserFolder, therefor get can't get entities")
        };

        let hierarchy_path = self
            .file
            .as_ref()
            .ok_or(Error::DatabaseNotSetup)
            .unwrap()
            .get_hierarchy_path_from_model(model)
            .expect("to find hierarchy path for model");

        let hierarchy = self
            .file
            .as_ref()
            .ok_or(Error::DatabaseNotSetup)
            .unwrap()
            .get_hierarchy(hierarchy_path)
            .expect("to get valid hierarchy for hierachy_path");

        hierarchy
            .children
            .as_ref()
            .expect("hierarchy to have children")
            .into_iter()
            .map(|hierarchy| hierarchy.id.clone().to_inner())
            .collect::<Vec<String>>()
    }

    #[method]
    fn get_entities_from_folder(&self, folder_id: String) -> Vec<ArticyModel<'_>> {
        self.get_entity_ids_from_folder(folder_id)
            .into_iter()
            .map(|id| {
                self.get_model(id)
                    .expect("to find model for id that is part of entity folder")
            })
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
    fn print_state(&self) {
        let state = &self
            .interpreter
            .as_ref()
            .ok_or(Error::InterpreterNotSetup)
            .unwrap()
            .state;

        godot_print!("{state:#?}");
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
                template,
                ..
            } => {
                let dictionary = Dictionary::new();
                dictionary.insert("line", text.to_owned());
                dictionary.insert("id", id.to_inner());
                dictionary.insert("speaker", speaker.to_inner());
                dictionary.insert("technical_name", technical_name.to_owned());

                if let Some(template) = template {
                    let json = unsafe {
                        gdnative::api::JSON::godot_singleton()
                            .parse(serde_json::to_string(template).unwrap())
                            .unwrap()
                            .assume_safe()
                            .result()
                    };

                    dictionary.insert("template", json);
                }

                owner.emit_signal("line", &[Variant::new(dictionary)]);
            }
            model => {
                owner.emit_signal("model", &[ArticyModel(model).to_variant()]);
            }
        }
    }

    #[method]
    fn advance(&mut self, #[base] owner: &Node) {
        match self
            .interpreter
            .as_mut()
            .ok_or(Error::InterpreterNotSetup)
            .unwrap()
            .advance()
        {
            Ok(outcome) => handle_outcome(owner, outcome),
            Err(error) => godot_error!("Got an error from using Interpreter.advance(): {error:#?}"),
        }
    }

    #[method]
    fn choose(&mut self, #[base] owner: &Node, id: String) {
        let interpreter = self
            .interpreter
            .as_mut()
            .ok_or(Error::InterpreterNotSetup)
            .unwrap();

        match interpreter.choose(Id(id)) {
            Ok(outcome) => handle_outcome(owner, outcome),
            Err(error) => godot_error!("Got an error from using Interpreter.choose(): {error:#?}"),
        }
    }

    #[method]
    fn exhaust_maximally(&mut self, #[base] owner: &Node) {
        let interpreter = self
            .interpreter
            .as_mut()
            .ok_or(Error::InterpreterNotSetup)
            .unwrap();

        interpreter
            .exhaust_maximally()
            .map_err(Error::ArticyError)
            .unwrap();

        self.advance(owner)
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

fn handle_outcome(owner: &Node, outcome: Outcome) {
    match outcome {
        Outcome::Advanced(Model::DialogueFragment {
            id,
            text,
            speaker,
            technical_name,
            template,
            ..
        }) => {
            let dictionary = Dictionary::new();

            dictionary.insert("id", id.to_inner());
            dictionary.insert("line", text.to_owned());
            dictionary.insert("speaker", speaker.to_inner());
            dictionary.insert("technical_name", technical_name.to_owned());

            if let Some(template) = template {
                let json = unsafe {
                    gdnative::api::JSON::godot_singleton()
                        .parse(serde_json::to_string(template).unwrap())
                        .unwrap()
                        .assume_safe()
                        .result()
                };

                dictionary.insert("template", json);
            }

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
                    Model::DialogueFragment {
                        menu_text,
                        id,
                        text,
                        ..
                    } => {
                        dictionary.insert(
                            "label",
                            if menu_text.is_empty() {
                                text
                            } else {
                                menu_text
                            }
                            .to_owned(),
                        );
                        dictionary.insert("id", id.to_inner());

                        array.push(dictionary);
                    }
                    other_model => {
                        owner.emit_signal("model", &[ArticyModel(other_model).to_variant()]);
                    }
                }
            }

            owner.emit_signal("choices", &[Variant::new(array)]);
        }
        Outcome::Stopped | Outcome::EndOfDialogue => {
            owner.emit_signal("stopped", &[]);
        }
    }
}

fn init(handle: InitHandle) {
    handle.add_tool_class::<Database>();
    handle.add_class::<Interpreter>();
}

godot_init!(init);
