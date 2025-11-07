use headwind::models::HelmRepository;
use kube::CustomResourceExt;

fn main() {
    print!("{}", serde_yaml::to_string(&HelmRepository::crd()).unwrap());
}
