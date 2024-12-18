use bevy::render::render_resource::encase::internal::WriteInto;
use bevy::render::view::{ViewUniform, ViewUniformOffset, ViewUniforms};
use bevy::{
    core_pipeline::{
        core_3d::graph::{Core3d, Node3d},
    },
    ecs::query::QueryItem,
    prelude::*,
    render::{
        extract_component::{
            ComponentUniforms, DynamicUniformIndex, ExtractComponent, ExtractComponentPlugin,
            UniformComponentPlugin,
        },
        render_graph::{
            NodeRunError, RenderGraphApp, RenderGraphContext, RenderLabel, ViewNode, ViewNodeRunner,
        },
        render_resource::{
            binding_types::{sampler, texture_2d, uniform_buffer},
            *,
        },
        renderer::{RenderContext, RenderDevice},
        view::ViewTarget,
        RenderApp,
    },
};
use std::fmt::Debug;
use std::hash::Hash;
use std::marker::PhantomData;

/// It is generally encouraged to set up post processing effects as a plugin
pub struct PostProcessPlugin<U: Clone, R: Debug + Hash + PartialEq + Eq + Clone + RenderLabel> {
    post_process_plugin_settings: PostProcessPluginSettings<U, R>,
}

impl<U: Clone, R: Debug + Hash + PartialEq + Eq + Clone + RenderLabel> PostProcessPlugin<U, R> {
    pub fn new(
        shader_path: &'static str,
        label: R,
        debug_label: Option<&'static str>,
        bind_group_layout_label: &'static str,
        vertex_state: VertexState,
    ) -> Self {
        Self {
            post_process_plugin_settings: PostProcessPluginSettings::<U, R> {
                shader_path,
                label,
                debug_label,
                bind_group_layout_label,
                vertex_state,
                phantom_data: PhantomData,
            },
        }
    }
}

impl<
        U: WriteInto + Component + ShaderType + Clone + ExtractComponent,
        R: Debug + Hash + PartialEq + Eq + Clone + RenderLabel,
    > Plugin for PostProcessPlugin<U, R>
{
    fn build(&self, app: &mut App) {
        app.add_plugins((
            // The settings will be a component that lives in the main world but will
            // be extracted to the render world every frame.
            // This makes it possible to control the effect from the main world.
            // This plugin will take care of extracting it automatically.
            // It's important to derive [`ExtractComponent`] on the `ShaderUniform`
            // for this plugin to work correctly.
            ExtractComponentPlugin::<U>::default(),
            // The settings will also be the data used in the shader.
            // This plugin will prepare the component for the GPU by creating a uniform buffer
            // and writing the data to that buffer every frame.
            UniformComponentPlugin::<U>::default(),
        ));

        // We need to get the render app from the main app
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };

        render_app
            // Bevy's renderer uses a render graph which is a collection of nodes in a directed acyclic graph.
            // It currently runs on each view/camera and executes each node in the specified order.
            // It will make sure that any node that needs a dependency from another node
            // only runs when that dependency is done.
            //
            // Each node can execute arbitrary work, but it generally runs at least one render pass.
            // A node only has access to the render world, so if you need data from the main world
            // you need to extract it manually or with the plugin like above.
            // Add a [`Node`] to the [`RenderGraph`]
            // The Node needs to impl FromWorld
            //
            // The [`ViewNodeRunner`] is a special [`Node`] that will automatically run the node for each view
            // matching the [`ViewQuery`]
            .add_render_graph_node::<ViewNodeRunner<PipelineNode<U, R>>>(
                // Specify the label of the graph, in this case we want the graph for 3d
                Core3d,
                // It also needs the label of the node
                self.post_process_plugin_settings.label.clone(),
            )
            .add_render_graph_edges(
                Core3d,
                // Specify the node ordering.
                // This will automatically create all required node edges to enforce the given ordering.
                (
                    Node3d::EndMainPass,
                    self.post_process_plugin_settings.label.clone(),
                    Node3d::EndMainPassPostProcessing,
                ),
            );
    }

    fn finish(&self, app: &mut App) {
        // We need to get the render app from the main app
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };

        render_app.insert_resource(self.post_process_plugin_settings.clone());

        render_app
            // Initialize the pipeline
            .init_resource::<PostProcessPipeline<U, R>>();
    }
}

#[derive(Resource, Clone)]
struct PostProcessPluginSettings<U, R: Debug + Hash + PartialEq + Eq + Clone + RenderLabel>
where
    U: Clone,
{
    shader_path: &'static str,
    /// Label that uniquely identifies this pipeline
    label: R,
    /// Debug label of the render pass. This will show up in graphics debuggers for easy identification.
    debug_label: Option<&'static str>,
    bind_group_layout_label: &'static str,
    vertex_state: VertexState,
    phantom_data: PhantomData<U>,
}

// The post process node used for the render graph
struct PipelineNode<U, R>(PhantomData<U>, PhantomData<R>);

impl<U, R> FromWorld for PipelineNode<U, R> {
    fn from_world(_world: &mut World) -> Self {
        Self(Default::default(), Default::default())
    }
}

// The ViewNode trait is required by the ViewNodeRunner
impl<
        U: Component + ShaderType + WriteInto + Clone,
        R: Send + Sync + 'static + Hash + Eq + Clone + RenderLabel,
    > ViewNode for PipelineNode<U, R>
{
    // The node needs a query to gather data from the ECS in order to do its rendering,
    // but it's not a normal system so we need to define it manually.
    //
    // This query will only run on the view entity
    type ViewQuery = (
        &'static ViewTarget,
        // This makes sure the node only runs on cameras with the SkyPipelineSettings component
        &'static U,
        &'static ViewUniformOffset,
        // As there could be multiple post processing components sent to the GPU (one per camera),
        // we need to get the index of the one that is associated with the current view.
        &'static DynamicUniformIndex<U>,
    );

    // Runs the node logic
    // This is where you encode draw commands.
    //
    // This will run on every view on which the graph is running.
    // If you don't want your effect to run on every camera,
    // you'll need to make sure you have a marker component as part of [`ViewQuery`]
    // to identify which camera(s) should run the effect.
    fn run(
        &self,
        _graph: &mut RenderGraphContext,
        render_context: &mut RenderContext,
        (
            view_target,
            _post_process_settings,
            view_uniform_offset,
            settings_index,
        ): QueryItem<Self::ViewQuery>,
        world: &World,
    ) -> Result<(), NodeRunError> {
        // Get the pipeline resource that contains the global data we need
        // to create the render pipeline
        let post_process_pipeline = world.resource::<PostProcessPipeline<U, R>>();

        // The pipeline cache is a cache of all previously created pipelines.
        // It is required to avoid creating a new pipeline each frame,
        // which is expensive due to shader compilation.
        let pipeline_cache = world.resource::<PipelineCache>();

        // Get the pipeline from the cache
        let Some(pipeline) = pipeline_cache.get_render_pipeline(post_process_pipeline.pipeline_id)
        else {
            return Ok(());
        };

        // Get the settings uniform binding
        let settings_uniforms = world.resource::<ComponentUniforms<U>>();
        let Some(settings_binding) = settings_uniforms.uniforms().binding() else {
            return Ok(());
        };

        let view_uniforms = world.resource::<ViewUniforms>();

        let Some(view_binding) = view_uniforms.uniforms.binding() else {
            return Ok(());
        };

        // This will start a new "post process write", obtaining two texture
        // views from the view target - a `source` and a `destination`.
        // `source` is the "current" main texture and you _must_ write into
        // `destination` because calling `post_process_write()` on the
        // [`ViewTarget`] will internally flip the [`ViewTarget`]'s main
        // texture to the `destination` texture. Failing to do so will cause
        // the current main texture information to be lost.
        let post_process = view_target.post_process_write();

        let plugin_settings = world
            .get_resource::<PostProcessPluginSettings<U, R>>()
            .unwrap();

        // The bind_group gets created each frame.
        //
        // Normally, you would create a bind_group in the Queue set,
        // but this doesn't work with the post_process_write().
        // The reason it doesn't work is because each post_process_write will alternate the source/destination.
        // The only way to have the correct source/destination for the bind_group
        // is to make sure you get it during the node execution.
        let bind_group = render_context.render_device().create_bind_group(
            plugin_settings.bind_group_layout_label,
            &post_process_pipeline.layout,
            // It's important for this to match the BindGroupLayout defined in the SkyPipelinePipeline
            &BindGroupEntries::sequential((
                // Make sure to use the source view
                post_process.source,
                // Use the sampler created for the pipeline
                &post_process_pipeline.sampler,
                // Set the settings binding
                settings_binding.clone(),
                view_binding.clone(),
            )),
        );

        // Begin the render pass
        let mut render_pass = render_context.begin_tracked_render_pass(RenderPassDescriptor {
            label: plugin_settings.debug_label,
            color_attachments: &[Some(RenderPassColorAttachment {
                // We need to specify the post process destination view here
                // to make sure we write to the appropriate texture.
                view: post_process.destination,
                resolve_target: None,
                ops: Operations::default(),
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        // This is mostly just wgpu boilerplate for drawing a fullscreen triangle,
        // using the pipeline/bind_group created above
        render_pass.set_render_pipeline(pipeline);
        // By passing in the index of the post process settings on this view, we ensure
        // that in the event that multiple settings were sent to the GPU (as would be the
        // case with multiple cameras), we use the correct one.
        render_pass.set_bind_group(
            0,
            &bind_group,
            &[settings_index.index(), view_uniform_offset.offset],
        );
        render_pass.draw(0..3, 0..1);

        Ok(())
    }
}

// This contains global data used by the render pipeline. This will be created once on startup.
#[derive(Resource)]
struct PostProcessPipeline<U, R> {
    layout: BindGroupLayout,
    sampler: Sampler,
    pipeline_id: CachedRenderPipelineId,
    _uniform: PhantomData<U>,
    _render_label: PhantomData<R>,
}

impl<U: Clone + Send + Sync + ShaderType + 'static, R: Hash + Eq + Clone + RenderLabel> FromWorld
    for PostProcessPipeline<U, R>
{
    fn from_world(world: &mut World) -> Self {
        let plugin_settings = world
            .get_resource::<PostProcessPluginSettings<U, R>>()
            .unwrap()
            .clone();
        let render_device = world.resource::<RenderDevice>();
        // We need to define the bind group layout used for our pipeline
        let layout = render_device.create_bind_group_layout(
            plugin_settings.bind_group_layout_label,
            &BindGroupLayoutEntries::sequential(
                // The layout entries will only be visible in the fragment stage
                ShaderStages::VERTEX_FRAGMENT,
                (
                    // The screen texture
                    texture_2d(TextureSampleType::Float { filterable: true }),
                    // The sampler that will be used to sample the screen texture
                    sampler(SamplerBindingType::Filtering),
                    // The settings uniform that will control the effect
                    uniform_buffer::<U>(true),
                    // The view uniform
                    uniform_buffer::<ViewUniform>(true),
                ),
            ),
        );

        // We can create the sampler here since it won't change at runtime and doesn't depend on the view
        let sampler = render_device.create_sampler(&SamplerDescriptor::default());

        // Get the shader handle
        let shader = world.load_asset(plugin_settings.shader_path);

        let pipeline_id = world
            .resource_mut::<PipelineCache>()
            // This will add the pipeline to the cache and queue its creation
            .queue_render_pipeline(RenderPipelineDescriptor {
                label: plugin_settings.debug_label.map(Into::into),
                layout: vec![layout.clone()],
                // This will setup a fullscreen triangle for the vertex state
                vertex: plugin_settings.vertex_state,
                fragment: Some(FragmentState {
                    shader,
                    shader_defs: vec![],
                    // Make sure this matches the entry point of your shader.
                    // It can be anything as long as it matches here and in the shader.
                    entry_point: "fragment".into(),
                    targets: vec![Some(ColorTargetState {
                        format: TextureFormat::bevy_default(),
                        blend: None,
                        write_mask: ColorWrites::ALL,
                    })],
                }),
                // All the following properties are not important for this effect so just use the default values.
                // This struct doesn't have the Default trait implemented because not all fields can have a default value.
                primitive: PrimitiveState::default(),
                depth_stencil: None,
                multisample: MultisampleState::default(),
                push_constant_ranges: vec![],
                zero_initialize_workgroup_memory: false,
            });

        PostProcessPipeline::<U, R> {
            layout,
            sampler,
            pipeline_id,
            _uniform: Default::default(),
            _render_label: Default::default(),
        }
    }
}
