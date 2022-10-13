use std::time::Duration;

use rand::prelude::IteratorRandom;

pub struct ContainerHandle {
    container_id: String,
    pub url: Option<String>,
    pub port_mapping: Option<(u16, u16)>,
    pub name: Option<String>,
}

impl Drop for ContainerHandle {
    fn drop(&mut self) {
        log::debug!(
            "docker stop {} {:#?} {:#?}",
            &self.container_id,
            &self.url,
            &self.name
        );

        std::process::Command::new("docker")
            .arg("stop")
            .arg(self.container_id.trim())
            .output()
            .unwrap();
    }
}

#[derive(Default)]
pub struct Builder {
    image: String,
    host: String,
    protocol: Option<String>,
    port_mapping: Option<(u16, u16)>,
    name: Option<String>,
}

impl Builder {
    pub fn new(image: &str) -> Self {
        Builder {
            image: image.to_string(),
            host: "localhost".to_string(),
            protocol: None,
            port_mapping: None,
            name: None,
        }
    }

    pub fn port_mapping(mut self, host_port: u16, port: Option<u16>) -> Self {
        let port = port.unwrap_or(host_port);
        self.port_mapping = Some((host_port, port));
        self
    }

    pub fn name(mut self, name: &str) -> Self {
        self.name = Some(name.into());
        self
    }

    pub async fn build_disposable(self) -> ContainerHandle {
        let mut args: Vec<String> = vec![];

        if let Some((host_port, port)) = &self.port_mapping {
            args.append(&mut vec!["-p".into(), format!("{}:{}", host_port, port)]);
        }

        if let Some(name) = &self.name {
            args.append(&mut vec!["--name".into(), format!("{}", name)]);
        }

        let protocol = self.protocol.or_else(|| match self.image.as_str() {
            "mongo" => Some("mongodb".to_owned()),
            "redis" => Some("redis".to_owned()),
            _ => None,
        });

        args.push(self.image);

        let mut try_times = 3;
        let launch_output = loop {
            let output = tokio::process::Command::new("docker")
                .args(["run", "--rm", "-d"])
                .args(&args)
                .output()
                .await
                .expect("Failed to launch docker container");

            if output.status.code().unwrap() == 0 {
                break output;
            }

            try_times -= 1;
            if try_times == 0 {
                panic!(
                    "Failed to launch docker container with {:#?}: {}, out-{:#?} err-{:#?}",
                    args, output.status, output.stdout, output.stderr
                )
            }

            tokio::time::sleep(Duration::from_millis(
                (30..3000).choose(&mut rand::thread_rng()).unwrap(),
            ))
            .await;
        };

        let container_id = String::from_utf8(launch_output.stdout)
            .unwrap()
            .trim()
            .to_string();

        let port_mapping = if let Some((0, port)) = self.port_mapping {
            let output = tokio::process::Command::new("docker")
                .args(["port", &container_id, port.to_string().as_str()])
                .output()
                .await
                .unwrap();

            let host_port_string = String::from_utf8(output.stdout).unwrap().trim().to_string();
            host_port_string
                .find(":")
                .map(|i| (host_port_string[(i + 1)..].parse::<u16>().unwrap(), port))
        } else {
            self.port_mapping
        };

        let url = protocol
            .map(|protocol| format!("{}://{}", protocol, self.host))
            .map(|uri| {
                if let Some((host_port, _)) = port_mapping {
                    format!("{}:{}/", uri, host_port)
                } else {
                    format!("{}/", uri)
                }
            });

        ContainerHandle {
            container_id,
            url,
            port_mapping,
            name: self.name,
        }
    }
}

#[cfg(test)]
mod tests {
    use bollard::container::InspectContainerOptions;
    use bollard::service::ContainerInspectResponse;

    use super::*;

    #[tokio::test]
    async fn test_build_docker_handle() {
        let docker = bollard::Docker::connect_with_local_defaults().unwrap();
        let name = "test_mongo";
        let (host_port, port) = (28017u16, 27017u16);

        {
            let handle = Builder::new("mongo")
                .port_mapping(host_port, Some(port))
                .name(name)
                .build_disposable()
                .await;

            let option = InspectContainerOptions { size: false };
            let info_opt = docker.inspect_container(name, Some(option)).await;
            assert!(info_opt.is_ok());

            let info = info_opt.unwrap();
            let expected_host_port = host_port_by_inspect_response(&info);
            let expected_url = format!("mongodb://localhost:{host_port}/");

            assert_eq!(host_port, expected_host_port);
            assert_eq!(info.id.unwrap(), handle.container_id);
            assert_eq!(handle.url.as_ref().unwrap().to_string(), expected_url);
            assert_eq!(
                handle.port_mapping.as_ref().unwrap(),
                &(expected_host_port, port)
            );
            assert_eq!(handle.name.as_ref().unwrap(), name);
        }

        // assert the container is stopped automatically after the handle destroy
        let option = InspectContainerOptions { size: false };
        let info_opt = docker.inspect_container(name, Some(option)).await;
        assert!(info_opt.is_err());
    }

    #[tokio::test]
    async fn test_build_docker_handle_with_auto_port() {
        let docker = bollard::Docker::connect_with_local_defaults().unwrap();
        let name = "test_mongo";

        {
            let handle = Builder::new("mongo")
                .port_mapping(0, Some(27017))
                .name(name)
                .build_disposable()
                .await;

            let option = InspectContainerOptions { size: false };
            let info_opt = docker.inspect_container(name, Some(option)).await;
            // assert the container is running
            assert!(info_opt.is_ok());

            let info = info_opt.unwrap();
            let expected_host_port = host_port_by_inspect_response(&info);
            let expected_url = format!("mongodb://localhost:{expected_host_port}/");

            assert_eq!(info.id.unwrap(), handle.container_id);
            assert_eq!(handle.url.as_ref().unwrap().to_string(), expected_url);
            assert_eq!(
                handle.port_mapping.as_ref().unwrap(),
                &(expected_host_port, 27017)
            );
            assert_eq!(handle.name.as_ref().unwrap(), name);
        }

        let option = InspectContainerOptions { size: false };
        let info_opt = docker.inspect_container(name, Some(option)).await;
        // assert the container is stopped automatically after the handle destroy
        assert!(info_opt.is_err());
    }

    fn host_port_by_inspect_response(info: &ContainerInspectResponse) -> u16 {
        info.network_settings
            .as_ref()
            .unwrap()
            .ports
            .as_ref()
            .unwrap()
            .get("27017/tcp")
            .unwrap()
            .as_ref()
            .unwrap()
            .get(0)
            .unwrap()
            .host_port
            .as_ref()
            .unwrap()
            .parse::<u16>()
            .unwrap()
    }
}
