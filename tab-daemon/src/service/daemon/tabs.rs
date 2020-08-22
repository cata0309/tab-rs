use super::tab::TabService;
use crate::bus::TabBus;
use crate::{
    bus::DaemonBus,
    message::{
        daemon::{CloseTab, CreateTab},
        tab::{TabRecv, TabSend},
    },
    state::tab::TabsState,
};
use log::{debug, info};
use std::{collections::HashMap};
use tab_api::tab::{TabId, TabMetadata};
use tab_service::{dyn_bus::DynBus, Bus, Lifeline, Service};
use tokio::{stream::StreamExt, sync::broadcast};

pub struct TabsService {
    _run: Lifeline,
}

enum TabEvent {
    Create(CreateTab),
    Close(CloseTab),
    TabSend(Result<TabSend, broadcast::RecvError>),
}

impl TabEvent {
    pub fn create(create: CreateTab) -> Self {
        TabEvent::Create(create)
    }

    pub fn close(close: CloseTab) -> Self {
        TabEvent::Close(close)
    }

    pub fn tabsend(tabsend: Result<TabSend, broadcast::RecvError>) -> Self {
        TabEvent::TabSend(tabsend)
    }
}

impl Service for TabsService {
    type Bus = DaemonBus;
    type Lifeline = anyhow::Result<Self>;

    fn spawn(bus: &Self::Bus) -> Self::Lifeline {
        let rx_create = bus.rx::<CreateTab>()?;
        let rx_exit = bus.rx::<CloseTab>()?;
        let rx_tab_send = bus.rx::<TabSend>()?;

        let tx_tab = bus.tx::<TabRecv>()?;
        let tx_tabs_state = bus.tx::<TabsState>()?;

        let tab_bus = TabBus::default();
        tab_bus.take_channel::<TabRecv, DaemonBus>(bus)?;
        tab_bus.take_tx::<TabSend, DaemonBus>(bus)?;

        let _run = Self::try_task("run", async move {
            let mut tabs: HashMap<TabId, TabMetadata> = HashMap::new();
            let mut lifelines: HashMap<TabId, TabService> = HashMap::new();

            let mut stream = rx_create
                .map(TabEvent::create)
                .merge(rx_exit.map(TabEvent::close))
                .merge(rx_tab_send.map(TabEvent::tabsend));

            while let Some(msg) = stream.next().await {
                match msg {
                    TabEvent::Create(create) => {
                        debug!("received create tab event: {:?}", &create);
                        let metadata = create.0;
                        if let Some(_) = tabs.values().find(|tab| metadata.name == tab.name) {
                            info!("tab {} already exists", metadata.name.as_str());
                            continue;
                        }

                        let tab = TabService::spawn(&tab_bus)?;
                        info!("tab {} pending, name {}", tab.id, metadata.name.as_str());

                        let metadata = TabMetadata {
                            id: tab.id,
                            name: metadata.name,
                            dimensions: metadata.dimensions.clone(),
                        };

                        tx_tab
                            .send(TabRecv::Init(metadata.clone()))
                            .map_err(|_e| anyhow::Error::msg("TabSend tx"))?;

                        tabs.insert(tab.id, metadata);
                        lifelines.insert(tab.id, tab);

                        tx_tabs_state.broadcast(TabsState::new(&tabs))?;
                    }
                    TabEvent::Close(close) => {
                        tabs.remove(&close.0);
                        lifelines.remove(&close.0);

                        tx_tabs_state.broadcast(TabsState::new(&tabs))?;
                    }
                    TabEvent::TabSend(event) => match event? {
                        TabSend::Stopped(id) => {
                            tabs.remove(&id);
                            lifelines.remove(&id);

                            tx_tabs_state.broadcast(TabsState::new(&tabs))?;
                        }
                        _ => {}
                    },
                }
            }

            Ok(())
        });

        Ok(Self { _run })
    }
}