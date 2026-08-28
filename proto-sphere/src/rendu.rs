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
    /// Gardée pour la relecture : filmer, c'est redescendre l'image du GPU.
    texture: wgpu::Texture,
}

impl Cible {
    pub fn nouvelle(
        device: &wgpu::Device,
        renderer: &mut egui_wgpu::Renderer,
        taille: (u32, u32),
    ) -> Self {
        let (texture, couleur, profondeur) = textures(device, taille);
        let id = renderer.register_native_texture(device, &couleur, wgpu::FilterMode::Linear);
        Self { taille, couleur, profondeur, id, texture }
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
        let (texture, couleur, profondeur) = textures(device, taille);
        self.texture = texture;
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

/// Une cible de coulisses : même chose, sans egui.
///
/// C'est là que le passé est peint avant que la nappe du portail ne
/// l'échantillonne. Personne ne la regarde directement, donc elle n'a ni
/// `TextureId` ni relecture — seulement de quoi être rendue puis lue par un
/// shader.
pub struct Coulisse {
    pub taille: (u32, u32),
    pub couleur: wgpu::TextureView,
    pub profondeur: wgpu::TextureView,
    /// Gardée pour la relecture : le film compare ce que la fenêtre montrait à
    /// ce qu'on a obtenu en la franchissant.
    texture: wgpu::Texture,
}

impl Coulisse {
    pub fn nouvelle(device: &wgpu::Device, taille: (u32, u32)) -> Self {
        let (texture, couleur, profondeur) = textures(device, taille);
        Self { taille, couleur, profondeur, texture }
    }

    pub fn ajuster(&mut self, device: &wgpu::Device, taille: (u32, u32)) {
        let taille = (taille.0.max(1), taille.1.max(1));
        if taille == self.taille {
            return;
        }
        let (texture, couleur, profondeur) = textures(device, taille);
        self.taille = taille;
        self.couleur = couleur;
        self.profondeur = profondeur;
        self.texture = texture;
    }

    pub fn relire(&self, device: &wgpu::Device, queue: &wgpu::Queue) -> Vec<u8> {
        relire_texture(device, queue, &self.texture, self.taille)
    }
}

fn textures(
    device: &wgpu::Device,
    (l, h): (u32, u32),
) -> (wgpu::Texture, wgpu::TextureView, wgpu::TextureView) {
    let dim = wgpu::Extent3d { width: l, height: h, depth_or_array_layers: 1 };

    let couleur = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("cible couleur"),
        size: dim,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT_COULEUR,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC,
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
        couleur.clone(),
        couleur.create_view(&wgpu::TextureViewDescriptor::default()),
        profondeur.create_view(&wgpu::TextureViewDescriptor::default()),
    )
}

impl Cible {
    /// Redescend l'image du GPU, en octets RGBA.
    ///
    /// La largeur est choisie multiple de 64 par le mode film, ce qui rend la
    /// ligne multiple de 256 octets et évite d'avoir à dépadder.
    pub fn relire(&self, device: &wgpu::Device, queue: &wgpu::Queue) -> Vec<u8> {
        relire_texture(device, queue, &self.texture, self.taille)
    }
}

fn relire_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    taille: (u32, u32),
) -> Vec<u8> {
    {
        let (l, h) = taille;
        let ligne = l * 4;
        assert_eq!(ligne % 256, 0, "largeur non alignée pour la relecture");

        let tampon = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("relecture"),
            size: (ligne * h) as u64,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let mut encodeur =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        encodeur.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &tampon,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(ligne),
                    rows_per_image: Some(h),
                },
            },
            wgpu::Extent3d { width: l, height: h, depth_or_array_layers: 1 },
        );
        queue.submit(Some(encodeur.finish()));

        let tranche = tampon.slice(..);
        tranche.map_async(wgpu::MapMode::Read, |_| {});
        let _ = device.poll(wgpu::Maintain::Wait);
        let octets = tranche.get_mapped_range().to_vec();
        tampon.unmap();
        octets
    }
}
