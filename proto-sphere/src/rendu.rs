//! La cible hors-ecran dans laquelle les deux vues dessinent.
//!
//! egui tient la fenetre et le menu ; la scene est peinte a cote, dans sa
//! propre texture avec son tampon de profondeur, puis affichee comme une image.

pub const FORMAT_COULEUR: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
pub const FORMAT_PROFONDEUR: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

pub struct Cible {
    pub taille: (u32, u32),
    pub couleur: wgpu::TextureView,
    pub profondeur: wgpu::TextureView,
    pub id: egui::TextureId,
}

impl Cible {
    pub fn nouvelle(
        device: &wgpu::Device,
        renderer: &mut egui_wgpu::Renderer,
        taille: (u32, u32),
    ) -> Self {
        let (couleur, profondeur) = textures(device, taille);
        let id = renderer.register_native_texture(device, &couleur, wgpu::FilterMode::Linear);
        Self { taille, couleur, profondeur, id }
    }

    pub fn ajuster(
        &mut self,
        device: &wgpu::Device,
        renderer: &mut egui_wgpu::Renderer,
        taille: (u32, u32),
    ) {
        let taille = (taille.0.max(1), taille.1.max(1));
        if taille == self.taille {
            return;
        }
        let (couleur, profondeur) = textures(device, taille);
        renderer.update_egui_texture_from_wgpu_texture(
            device,
            &couleur,
            wgpu::FilterMode::Linear,
            self.id,
        );
        self.taille = taille;
        self.couleur = couleur;
        self.profondeur = profondeur;
    }
}

fn textures(
    device: &wgpu::Device,
    (l, h): (u32, u32),
) -> (wgpu::TextureView, wgpu::TextureView) {
    let dim = wgpu::Extent3d { width: l, height: h, depth_or_array_layers: 1 };

    let couleur = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("cible couleur"),
        size: dim,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT_COULEUR,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });

    let profondeur = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("cible profondeur"),
        size: dim,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT_PROFONDEUR,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });

    (
        couleur.create_view(&wgpu::TextureViewDescriptor::default()),
        profondeur.create_view(&wgpu::TextureViewDescriptor::default()),
    )
}
