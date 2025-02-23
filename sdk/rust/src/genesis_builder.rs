use {
    crate::AdminOption,
    anyhow::{anyhow, ensure},
    cw_std::{hash, to_json, Addr, Binary, Coins, Config, GenesisState, Hash, Message, GENESIS_SENDER},
    home::home_dir,
    lazy_static::lazy_static,
    serde::ser::Serialize,
    serde_json::Value,
    std::{
        fs,
        fs::File,
        io::Read,
        path::{Path, PathBuf},
    },
};

lazy_static! {
    // the default path to the CometBFT genesis file
    static ref DEFAULT_COMET_GEN_PATH: PathBuf = {
        home_dir().unwrap().join(".cometbft/config/genesis.json")
    };
}

/// Helper for building genesis state. See the examples folder of this repository
/// for an example.
#[derive(Default)]
pub struct GenesisBuilder {
    cfg:        Option<Config>,
    code_msgs:  Vec<Message>,
    other_msgs: Vec<Message>,
}

impl GenesisBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn store_code(&mut self, path: impl AsRef<Path>) -> anyhow::Result<Hash> {
        // read Wasm byte code from file
        let mut file = File::open(path)?;
        let mut wasm_byte_code = vec![];
        file.read_to_end(&mut wasm_byte_code)?;

        // compute hash
        let code_hash = hash(&wasm_byte_code);

        // push the message into queue
        self.code_msgs.push(Message::StoreCode {
            wasm_byte_code: wasm_byte_code.into(),
        });

        Ok(code_hash)
    }

    pub fn instantiate<M: Serialize>(
        &mut self,
        code_hash: Hash,
        msg:       M,
        salt:      Binary,
        admin:     AdminOption,
    ) -> anyhow::Result<Addr> {
        // note: we use an all-zero address as the message sender during genesis
        let contract = Addr::compute(&GENESIS_SENDER, &code_hash, &salt);
        let admin = admin.decide(&contract);
        self.other_msgs.push(Message::Instantiate {
            code_hash,
            msg: to_json(&msg)?,
            salt,
            funds: Coins::new_empty(),
            admin,
        });
        Ok(contract)
    }

    pub fn store_code_and_instantiate<M: Serialize>(
        &mut self,
        path:  impl AsRef<Path>,
        msg:   M,
        salt:  Binary,
        admin: AdminOption,
    ) -> anyhow::Result<Addr> {
        let code_hash = self.store_code(path)?;
        self.instantiate(code_hash, msg, salt, admin)
    }

    pub fn execute<M: Serialize>(&mut self, contract: Addr, msg: M) -> anyhow::Result<()> {
        self.other_msgs.push(Message::Execute {
            contract,
            msg:  to_json(&msg)?,
            funds: Coins::new_empty(),
        });

        Ok(())
    }

    pub fn set_config(&mut self, cfg: Config) -> anyhow::Result<()> {
        ensure!(self.cfg.is_none(), "Config is set twice. Something is probably wrong in your workflow");

        self.cfg = Some(cfg);

        Ok(())
    }

    fn finalize(mut self) -> anyhow::Result<GenesisState> {
        // config must have been set
        let config = self.cfg.take().ok_or(anyhow!("config is not set"))?;

        // ensure that store code messages are in front of all other msgs
        let mut msgs = self.code_msgs;
        msgs.extend(self.other_msgs);

        Ok(GenesisState { config, msgs })
    }

    pub fn write_to_file(self, comet_gen_path: Option<PathBuf>) -> anyhow::Result<()> {
        let comet_gen_path = comet_gen_path.unwrap_or_else(|| DEFAULT_COMET_GEN_PATH.to_path_buf());
        let comet_gen_str = fs::read_to_string(&comet_gen_path)?;
        let mut comet_gen: Value = serde_json::from_str(&comet_gen_str)?;

        let app_state = self.finalize()?;
        comet_gen["app_state"] = serde_json::to_value(app_state)?;

        fs::write(&comet_gen_path, serde_json::to_string_pretty(&comet_gen)?)?;

        Ok(())
    }
}
