use std::collections::HashMap;

use bollard::models::{ContainerInspectResponse, PortBinding};
use bollard::{
    container::{CreateContainerOptions, StartContainerOptions},
    service::HostConfig,
};

pub struct ContainerHandle {
    pub container_id: String,
    pub name: Option<String>,
    pub host_ip: String,
    pub default_host_port: Option<String>,
    pub protocol: Option<String>,
    docker: bollard::Docker,
}

impl ContainerHandle {
    pub fn url(&self) -> String {
        let protocol = self.protocol.as_ref().unwrap();
        match self.default_host_port.as_ref() {
            Some(port) => format!("{protocol}://{host}:{port}/", host = self.host_ip.as_str()),
            None => format!("{protocol}://{host}/", host = self.host_ip.as_str()),
        }
    }

    pub async fn url_by<S: AsRef<str>>(&self, port: S) -> Option<String> {
        let protocol = self.protocol.as_ref().unwrap();
        let port = port.as_ref();

        let info = self
            .docker
            .inspect_container(&self.container_id, None)
            .await
            .unwrap();
        let host_port = info.get_host_port(Some(self.host_ip.as_str()), port);
        host_port.map(|host_port| {
            format!(
                "{protocol}://{host}:{host_port}",
                host = self.host_ip.as_str()
            )
        })
    }
}

impl Drop for ContainerHandle {
    fn drop(&mut self) {
        std::process::Command::new("docker")
            .arg("stop")
            .arg(self.container_id.trim())
            .output()
            .unwrap();
    }
}

#[derive(Default)]
pub struct Builder {
    /// Container config
    config: bollard::container::Config<String>,
    /// Container create options
    create_options: Option<CreateContainerOptions<String>>,
    /// Default accessing protocol
    protocol: Option<String>,
    /// Default accessing port
    default_port: Option<String>,
}

impl Builder {
    pub fn new<S: Into<String>>(image: S) -> Self {
        let image = image.into();
        let protocol = match image.as_str() {
            "mongo" => Some("mongodb".to_owned()),
            "redis" => Some("redis".to_owned()),
            _ => None,
        };
        Builder {
            config: bollard::container::Config {
                image: Some(image),
                host_config: Some(HostConfig {
                    auto_remove: Some(true),
                    ..Default::default()
                }),
                ..Default::default()
            },
            create_options: None,
            protocol,
            default_port: None,
        }
    }

    pub fn bind_port<S, T>(mut self, host_port: Option<S>, port: T) -> Self
    where
        S: Into<String>,
        T: Into<String>,
    {
        let port = canonicalize_port(port.into());
        let host_ip = "localhost".to_string();
        let host_port = host_port.map(Into::into).unwrap_or(port.clone());
        let binding = PortBinding {
            host_ip: Some(host_ip),
            host_port: Some(host_port),
        };

        let host_config = self.host_config();
        if host_config.port_bindings.is_none() {
            host_config.port_bindings = Some(HashMap::new());
        }
        let port_bindings = host_config.port_bindings.as_mut().unwrap();
        if let Some(Some(bindings)) = port_bindings.get_mut(port.as_str()) {
            bindings.push(binding)
        } else {
            port_bindings.insert(port, Some(vec![binding]));
        }
        self
    }

    pub fn bind_port_as_default<S, T>(mut self, host_port: Option<S>, port: T) -> Self
    where
        S: Into<String>,
        T: Into<String>,
    {
        let port = canonicalize_port(port.into());
        self.default_port = Some(port.clone());
        self.bind_port(host_port, port.as_str())
    }

    #[deprecated(since = "0.2.0", note = "please use `bind_port`")]
    pub fn port_mapping(self, host_port: u16, port: Option<u16>) -> Self {
        let port = port.unwrap_or(host_port).to_string();
        let host_port = host_port.to_string();
        self.bind_port(Some(host_port), port)
    }

    pub fn bind_volume<S: Into<String>>(mut self, bind: S) -> Self {
        let host_config = self.host_config();
        if host_config.binds.is_none() {
            host_config.binds = Some(Vec::new());
        }
        host_config.binds.as_mut().unwrap().push(bind.into());
        self
    }

    pub fn name<S: Into<String>>(mut self, name: S) -> Self {
        self.create_options().name = name.into();
        self
    }

    pub fn protocol<S: Into<String>>(mut self, protocol: S) -> Self {
        self.protocol = Some(protocol.into());
        self
    }

    pub fn host_config(&mut self) -> &mut HostConfig {
        self.config.host_config.as_mut().unwrap()
    }

    fn create_options(&mut self) -> &mut CreateContainerOptions<String> {
        if self.create_options.is_none() {
            self.create_options = Some(Default::default())
        }
        self.create_options.as_mut().unwrap()
    }

    pub async fn build_disposable(self) -> ContainerHandle {
        let host_ip = "localhost".to_string();
        // should be consistent with host_ip
        let docker = bollard::Docker::connect_with_local_defaults().unwrap();
        let container_handle = docker
            .create_container(self.create_options, self.config)
            .await
            .unwrap();
        docker
            .start_container(&container_handle.id, None::<StartContainerOptions<String>>)
            .await
            .unwrap();
        let container_info = docker
            .inspect_container(&container_handle.id, None)
            .await
            .unwrap();

        let default_host_port = self
            .default_port
            .and_then(|port| container_info.get_host_port(Some(host_ip.as_str()), port.as_str()));

        ContainerHandle {
            container_id: container_handle.id,
            name: container_info.get_name(),
            host_ip,
            protocol: self.protocol,
            default_host_port,
            docker,
        }
    }
}

fn canonicalize_port<S: Into<String>>(port: S) -> String {
    let port = port.into();
    if port.contains('/') {
        port
    } else {
        port + "/tcp"
    }
}

trait ContainerInspectResponseExt {
    fn get_host_port<S: AsRef<str>>(&self, host_ip: Option<S>, port: S) -> Option<String>;
    fn get_name(&self) -> Option<String>;
}

impl ContainerInspectResponseExt for ContainerInspectResponse {
    fn get_host_port<S: AsRef<str>>(&self, host_ip: Option<S>, port: S) -> Option<String> {
        let port = canonicalize_port(port.as_ref().to_string());
        // the ip of localhost/127.0.0.1 will be canonicalized as 0.0.0.0 by docker
        let host_ip = host_ip.as_ref().map(|ip| {
            if ip.as_ref() == "localhost" || ip.as_ref() == "127.0.0.1" {
                "0.0.0.0"
            } else {
                ip.as_ref()
            }
        });

        if let Some(network_settings) = self.network_settings.as_ref() {
            if let Some(port_map) = network_settings.ports.as_ref() {
                for (container_port, bindings) in port_map {
                    if container_port != &port {
                        continue;
                    }

                    if let Some(bindings) = bindings {
                        for binding in bindings {
                            if binding.host_ip.as_ref().map(String::as_str) == host_ip {
                                return binding.host_port.clone();
                            }
                        }
                    }
                    return None;
                }
            }
        }
        None
    }

    fn get_name(&self) -> Option<String> {
        self.name.as_ref().map(|name| match name.strip_prefix("/") {
            Some(stripped) => stripped.to_string(),
            None => name.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use bollard::container::InspectContainerOptions;
    use fake::Fake;

    use super::*;

    #[tokio::test]
    async fn test_build_docker_handle() {
        let host_ip = "localhost";
        let docker = bollard::Docker::connect_with_local_defaults().unwrap();
        let name: String = fake::faker::lorem::en::Word().fake();
        let host_port = "28017";
        let port = "27017";

        {
            let handle = Builder::new("mongo")
                .bind_port_as_default(Some(host_port), port)
                .name(name.as_str())
                .build_disposable()
                .await;

            let option = InspectContainerOptions { size: false };
            let info_opt = docker.inspect_container(name.as_str(), Some(option)).await;
            assert!(info_opt.is_ok());

            let info = info_opt.unwrap();
            let expected_host_port = info.get_host_port(Some(host_ip), port);
            let expected_url = format!("mongodb://localhost:{host_port}/");

            assert_eq!(host_port, expected_host_port.as_ref().unwrap().as_str());
            assert_eq!(info.id.unwrap(), handle.container_id);
            assert_eq!(handle.url(), expected_url);
            assert_eq!(handle.default_host_port, expected_host_port);
            assert_eq!(handle.name.as_ref().unwrap(), &name);
        }

        // assert the container is stopped automatically after the handle destroy
        let option = InspectContainerOptions { size: false };
        let info_opt = docker.inspect_container(name.as_str(), Some(option)).await;
        assert!(info_opt.is_err());
    }

    #[tokio::test]
    async fn test_build_docker_handle_with_auto_port() {
        let host_ip = "localhost";
        let docker = bollard::Docker::connect_with_local_defaults().unwrap();
        let name: String = fake::faker::lorem::en::Word().fake();
        let host_port = "0";
        let port = "27017";

        {
            let handle = Builder::new("mongo")
                .bind_port_as_default(Some(host_port), port)
                .name(name.as_str())
                .build_disposable()
                .await;

            let option = InspectContainerOptions { size: false };
            let info_opt = docker.inspect_container(name.as_str(), Some(option)).await;
            // assert the container is running
            assert!(info_opt.is_ok());

            let info = info_opt.unwrap();
            let expected_host_port = info.get_host_port(Some(host_ip), port);
            let expected_url = format!(
                "mongodb://localhost:{host}/",
                host = expected_host_port.as_ref().unwrap()
            );

            assert_eq!(info.id.unwrap(), handle.container_id);
            assert_eq!(handle.url(), expected_url);
            assert_eq!(expected_host_port, handle.default_host_port);
            assert_eq!(handle.name.as_ref().unwrap(), &name);
        }

        let option = InspectContainerOptions { size: false };
        let info_opt = docker.inspect_container(name.as_str(), Some(option)).await;
        // assert the container is stopped automatically after the handle destroy
        assert!(info_opt.is_err());
    }
}
