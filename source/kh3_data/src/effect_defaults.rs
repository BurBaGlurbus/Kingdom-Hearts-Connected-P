//! What a cooked effect object leaves out: it serializes only what differs from its class default, and
//! those defaults live in the class constructors of the executable. The values the effect reader
//! reads and whose absence would change what it does, per class.

pub const CLASS_DEFAULTS: &[(&str, &str)] = &[
    ("ParticleEmitter", r#"{"EmitterRenderMode":"ERM_Normal"}"#),
    ("ParticleLODLevel", r#"{"bEnabled":true}"#),
    ("ParticleModule", r#"{"bEnabled":true}"#),
    (
        "ParticleModuleAcceleration",
        r#"{"bApplyOwnerScale":true,"bEnabled":true}"#,
    ),
    ("ParticleModuleAccelerationBase", r#"{"bEnabled":true}"#),
    ("ParticleModuleAccelerationConstant", r#"{"bEnabled":true}"#),
    ("ParticleModuleAccelerationDrag", r#"{"bEnabled":true}"#),
    (
        "ParticleModuleAccelerationDragScaleOverLife",
        r#"{"bEnabled":true}"#,
    ),
    (
        "ParticleModuleAccelerationOverLifetime",
        r#"{"bEnabled":true}"#,
    ),
    ("ParticleModuleAttractorBase", r#"{"bEnabled":true}"#),
    ("ParticleModuleAttractorLine", r#"{"bEnabled":true}"#),
    ("ParticleModuleAttractorParticle", r#"{"bEnabled":true}"#),
    (
        "ParticleModuleAttractorPoint",
        r#"{"Negative_Z":true,"Negative_Y":true,"Negative_X":true,"Positive_Z":true,"Positive_Y":true,"Positive_X":true,"StrengthByDistance":true,"bEnabled":true}"#,
    ),
    (
        "ParticleModuleAttractorPointGravity",
        r#"{"bEnabled":true}"#,
    ),
    ("ParticleModuleBeamBase", r#"{"bEnabled":true}"#),
    ("ParticleModuleBeamModifier", r#"{"bEnabled":true}"#),
    ("ParticleModuleBeamNoise", r#"{"bEnabled":true}"#),
    ("ParticleModuleBeamSource", r#"{"bEnabled":true}"#),
    ("ParticleModuleBeamTarget", r#"{"bEnabled":true}"#),
    ("ParticleModuleCameraBase", r#"{"bEnabled":true}"#),
    ("ParticleModuleCameraOffset", r#"{"bEnabled":true}"#),
    (
        "ParticleModuleCollision",
        r#"{"CollisionCompletionOption":"EPCC_Kill","bEnabled":true}"#,
    ),
    ("ParticleModuleCollisionBase", r#"{"bEnabled":true}"#),
    (
        "ParticleModuleCollisionGPU",
        r#"{"Response":"EParticleCollisionResponse::Bounce","RadiusScale":1,"bEnabled":true}"#,
    ),
    ("ParticleModuleColor", r#"{"bEnabled":true}"#),
    ("ParticleModuleColorBase", r#"{"bEnabled":true}"#),
    ("ParticleModuleColorOverLife", r#"{"bEnabled":true}"#),
    ("ParticleModuleColorScaleOverLife", r#"{"bEnabled":true}"#),
    ("ParticleModuleColor_Seeded", r#"{"bEnabled":true}"#),
    ("ParticleModuleEventBase", r#"{"bEnabled":true}"#),
    ("ParticleModuleEventGenerator", r#"{"bEnabled":true}"#),
    (
        "ParticleModuleEventReceiverBase",
        r#"{"EventGeneratorType":"EPET_Any","bEnabled":true}"#,
    ),
    (
        "ParticleModuleEventReceiverKillParticles",
        r#"{"EventGeneratorType":"EPET_Any","bEnabled":true}"#,
    ),
    (
        "ParticleModuleEventReceiverSpawn",
        r#"{"EventGeneratorType":"EPET_Any","bEnabled":true}"#,
    ),
    ("ParticleModuleKillBase", r#"{"bEnabled":true}"#),
    ("ParticleModuleKillBox", r#"{"bEnabled":true}"#),
    ("ParticleModuleKillHeight", r#"{"bEnabled":true}"#),
    ("ParticleModuleLifetime", r#"{"bEnabled":true}"#),
    ("ParticleModuleLifetimeBase", r#"{"bEnabled":true}"#),
    ("ParticleModuleLifetime_Seeded", r#"{"bEnabled":true}"#),
    ("ParticleModuleLight", r#"{"bEnabled":true}"#),
    ("ParticleModuleLightBase", r#"{"bEnabled":true}"#),
    ("ParticleModuleLight_Seeded", r#"{"bEnabled":true}"#),
    ("ParticleModuleLocation", r#"{"bEnabled":true}"#),
    ("ParticleModuleLocationBase", r#"{"bEnabled":true}"#),
    (
        "ParticleModuleLocationBoneSocket",
        r#"{"InheritVelocityScale":1,"bEnabled":true}"#,
    ),
    ("ParticleModuleLocationDirect", r#"{"bEnabled":true}"#),
    ("ParticleModuleLocationEmitter", r#"{"bEnabled":true}"#),
    (
        "ParticleModuleLocationEmitterDirect",
        r#"{"bEnabled":true}"#,
    ),
    (
        "ParticleModuleLocationPrimitiveBase",
        r#"{"Negative_Z":true,"Negative_Y":true,"Negative_X":true,"Positive_Z":true,"Positive_Y":true,"Positive_X":true,"bEnabled":true}"#,
    ),
    (
        "ParticleModuleLocationPrimitiveCylinder",
        r#"{"HeightAxis":"PMLPC_HEIGHTAXIS_Z","RadialVelocity":true,"Negative_Z":true,"Negative_Y":true,"Negative_X":true,"Positive_Z":true,"Positive_Y":true,"Positive_X":true,"bEnabled":true}"#,
    ),
    (
        "ParticleModuleLocationPrimitiveCylinder_Seeded",
        r#"{"HeightAxis":"PMLPC_HEIGHTAXIS_Z","RadialVelocity":true,"Negative_Z":true,"Negative_Y":true,"Negative_X":true,"Positive_Z":true,"Positive_Y":true,"Positive_X":true,"bEnabled":true}"#,
    ),
    (
        "ParticleModuleLocationPrimitiveSphere",
        r#"{"Negative_Z":true,"Negative_Y":true,"Negative_X":true,"Positive_Z":true,"Positive_Y":true,"Positive_X":true,"bEnabled":true}"#,
    ),
    (
        "ParticleModuleLocationPrimitiveSphere_Seeded",
        r#"{"Negative_Z":true,"Negative_Y":true,"Negative_X":true,"Positive_Z":true,"Positive_Y":true,"Positive_X":true,"bEnabled":true}"#,
    ),
    (
        "ParticleModuleLocationPrimitiveTriangle",
        r#"{"bEnabled":true}"#,
    ),
    (
        "ParticleModuleLocationSkelVertSurface",
        r#"{"InheritVelocityScale":1,"bEnabled":true}"#,
    ),
    ("ParticleModuleLocationWorldOffset", r#"{"bEnabled":true}"#),
    (
        "ParticleModuleLocationWorldOffset_Seeded",
        r#"{"bEnabled":true}"#,
    ),
    ("ParticleModuleLocation_Seeded", r#"{"bEnabled":true}"#),
    ("ParticleModuleMaterialBase", r#"{"bEnabled":true}"#),
    ("ParticleModuleMeshMaterial", r#"{"bEnabled":true}"#),
    ("ParticleModuleMeshRotation", r#"{"bEnabled":true}"#),
    ("ParticleModuleMeshRotationRate", r#"{"bEnabled":true}"#),
    (
        "ParticleModuleMeshRotationRateMultiplyLife",
        r#"{"bEnabled":true}"#,
    ),
    (
        "ParticleModuleMeshRotationRateOverLife",
        r#"{"bEnabled":true}"#,
    ),
    (
        "ParticleModuleMeshRotationRate_Seeded",
        r#"{"bEnabled":true}"#,
    ),
    ("ParticleModuleMeshRotation_Seeded", r#"{"bEnabled":true}"#),
    ("ParticleModuleOrbit", r#"{"bEnabled":true}"#),
    ("ParticleModuleOrbitBase", r#"{"bEnabled":true}"#),
    ("ParticleModuleOrientationAxisLock", r#"{"bEnabled":true}"#),
    ("ParticleModuleOrientationBase", r#"{"bEnabled":true}"#),
    ("ParticleModuleParameterBase", r#"{"bEnabled":true}"#),
    ("ParticleModuleParameterDynamic", r#"{"bEnabled":true}"#),
    (
        "ParticleModuleParameterDynamic_Seeded",
        r#"{"bEnabled":true}"#,
    ),
    ("ParticleModulePivotOffset", r#"{"bEnabled":true}"#),
    (
        "ParticleModuleRequired",
        r#"{"MaxDrawCount":500,"bUseMaxDrawCount":true,"SubImages_Vertical":1,"SubImages_Horizontal":1,"EmitterDuration":1,"bUseLegacyEmitterTime":true,"EmitterDurationTime":1,"EmitterLoopDuration":0.8,"EmitterLoopTime":0.2,"ScreenAlignment":"PSA_Square","RotationPriority":"ROTATION_PRIORITY_XZY","bAutoAddToComponent":true,"EmitterBlendType":"BLEND_Opaque","bEnabled":true}"#,
    ),
    ("ParticleModuleRotation", r#"{"bEnabled":true}"#),
    ("ParticleModuleRotationBase", r#"{"bEnabled":true}"#),
    ("ParticleModuleRotationOverLifetime", r#"{"bEnabled":true}"#),
    ("ParticleModuleRotationRate", r#"{"bEnabled":true}"#),
    ("ParticleModuleRotationRateBase", r#"{"bEnabled":true}"#),
    (
        "ParticleModuleRotationRateMultiplyLife",
        r#"{"bEnabled":true}"#,
    ),
    ("ParticleModuleRotationRate_Seeded", r#"{"bEnabled":true}"#),
    ("ParticleModuleRotation_Seeded", r#"{"bEnabled":true}"#),
    ("ParticleModuleSQEX_2DFluidInflow", r#"{"bEnabled":true}"#),
    (
        "ParticleModuleSQEX_2DFluidParameters",
        r#"{"Force":1,"bEnabled":true}"#,
    ),
    ("ParticleModuleSQEX_AllEmitterFade", r#"{"bEnabled":true}"#),
    (
        "ParticleModuleSQEX_AllParticleKill",
        r#"{"KillEmitterTime":1,"bEnabled":true}"#,
    ),
    (
        "ParticleModuleSQEX_AnyVertexSpawn",
        r#"{"VartexGenerateType":"EVARTEX_ODER_TYPE_ORDER","HNumber":3,"WNumber":3,"ShapeType":"SQEX_ANY_VERTEX_SHAPE_TYPE_SPHERE","SpawnRotCalcMode":"ESQEX_SPAWN_ROT_CALC_MODE_TANGENT","bApplySpawnLocationOwnerScale":true,"bApplyOwnerScale":true,"DirectionType":"EPARTICLE_DIR_TYPE_EMITTER_NORMAL","NormalDirection":0,"bProcessBurstList":true,"bProcessSpawnRate":true,"bEnabled":true}"#,
    ),
    (
        "ParticleModuleSQEX_AnyVertexSpawn_Seeded",
        r#"{"VartexGenerateType":"EVARTEX_ODER_TYPE_ORDER","HNumber":3,"WNumber":3,"ShapeType":"SQEX_ANY_VERTEX_SHAPE_TYPE_SPHERE","SpawnRotCalcMode":"ESQEX_SPAWN_ROT_CALC_MODE_TANGENT","bApplySpawnLocationOwnerScale":true,"bApplyOwnerScale":true,"DirectionType":"EPARTICLE_DIR_TYPE_EMITTER_NORMAL","NormalDirection":0,"bProcessBurstList":true,"bProcessSpawnRate":true,"bEnabled":true}"#,
    ),
    (
        "ParticleModuleSQEX_AttachParticle",
        r#"{"bIsAttachRotation":true,"bIsAttachPosition":true,"bEnabled":true}"#,
    ),
    ("ParticleModuleSQEX_Base", r#"{"bEnabled":true}"#),
    (
        "ParticleModuleSQEX_BloomRadialBlur",
        r#"{"Mode":"SQEX_RADIAL_BLUR_MODE_RADIAL","DistanceFadeRate":500,"EffectPriority":"ScreenEffectPriority_Lowest","EndTime":1,"bEnabled":true}"#,
    ),
    (
        "ParticleModuleSQEX_BlurBase",
        r#"{"DistanceFadeRate":500,"EffectPriority":"ScreenEffectPriority_Lowest","bEnabled":true}"#,
    ),
    ("ParticleModuleSQEX_BoneAttach", r#"{"bEnabled":true}"#),
    (
        "ParticleModuleSQEX_BoxForce",
        r#"{"HitParticleSizeScale":1,"bEnabled":true}"#,
    ),
    (
        "ParticleModuleSQEX_CameraShake",
        r#"{"ShakeScaleDistanceMin":0.2,"ShakeScaleDistanceOffset":1000,"ShakeScaleDistance":2000,"bEnabled":true}"#,
    ),
    (
        "ParticleModuleSQEX_CapsuleForce",
        r#"{"HitParticleSizeScale":1,"bEnabled":true}"#,
    ),
    (
        "ParticleModuleSQEX_ChangeSceneColor",
        r#"{"EffectPriority":"ScreenEffectPriority_Lowest","bEnabled":true}"#,
    ),
    (
        "ParticleModuleSQEX_ComponentCutoff",
        r#"{"ComponentScaleTakeoverType":"SQEX_TAKEOVER_TYPE_ALWAYS","ComponentRotationTakeoverType":"SQEX_TAKEOVER_TYPE_ALWAYS","bFadeTakeoverType":true,"bEnabled":true}"#,
    ),
    (
        "ParticleModuleSQEX_CurlNoise",
        r#"{"TimeScaleAxisType":"ESQEX_CURLE_NOISE_TIME_SCALE_AXIS_TYPE_XYZ","TimeType":"ESQEX_CURLNOISE_TIME_SCALE_TYPE_EMITTER_TOTAL_TIME","bEnabled":true}"#,
    ),
    (
        "ParticleModuleSQEX_DepthOffset",
        r#"{"DepthOffset":50,"bEnabled":true}"#,
    ),
    ("ParticleModuleSQEX_EmitterDirectEX", r#"{"bEnabled":true}"#),
    (
        "ParticleModuleSQEX_EmitterInitializeTransforms",
        r#"{"bEnabled":true}"#,
    ),
    (
        "ParticleModuleSQEX_EmitterParentTailHistoryPos",
        r#"{"bEnabled":true}"#,
    ),
    (
        "ParticleModuleSQEX_EmitterTakeover",
        r#"{"ParticleSizeScaleTakeoverType":"SQEX_TAKEOVER_TYPE_ALWAYS","ParentParticleSizeScaleTakeoverType":"SQEX_TAKEOVER_TYPE_ALWAYS","ParentParticleAlphaTakeoverType":"SQEX_TAKEOVER_TYPE_ALWAYS","ParentParticleColorTakeoverType":"SQEX_TAKEOVER_TYPE_ALWAYS","ParentScaleTakeoverType":"SQEX_TAKEOVER_TYPE_ALWAYS","ParentRotationTakeoverType":"SQEX_TAKEOVER_TYPE_ALWAYS","ParentPositionTakeoverType":"SQEX_TAKEOVER_TYPE_ALWAYS","ComponentScaleTakeoverType":"SQEX_TAKEOVER_TYPE_ALWAYS","ComponentRotationTakeoverType":"SQEX_TAKEOVER_TYPE_ALWAYS","ComponentPositionTakeoverType":"SQEX_TAKEOVER_TYPE_ALWAYS","bFadeTakeoverType":true,"bEnabled":true}"#,
    ),
    (
        "ParticleModuleSQEX_EmitterTransforms",
        r#"{"bIsUseTransformScale":true,"bCurveParameterDirectScale":true,"bCurveParameterDirectPosition":true,"DisplayType":"DISPLAY_TYPE_NONE","bEnabled":true}"#,
    ),
    (
        "ParticleModuleSQEX_ExternalVertexSpawn",
        r#"{"SpawnRotCalcMode":"ESQEX_SPAWN_ROT_CALC_MODE_TANGENT","bApplySpawnLocationOwnerScale":true,"bApplyOwnerScale":true,"DirectionType":"EPARTICLE_DIR_TYPE_EMITTER_NORMAL","NormalDirection":1,"bProcessBurstList":true,"bProcessSpawnRate":true,"bEnabled":true}"#,
    ),
    (
        "ParticleModuleSQEX_Filter",
        r#"{"FildterType":"ESQEX_FILTER_TYPE_ALPHA","bEnabled":true}"#,
    ),
    ("ParticleModuleSQEX_FloatParameter", r#"{"bEnabled":true}"#),
    (
        "ParticleModuleSQEX_FloatParameterEx",
        r#"{"bEnabled":true}"#,
    ),
    ("ParticleModuleSQEX_Fluid", r#"{"bEnabled":true}"#),
    (
        "ParticleModuleSQEX_GaussBlur",
        r#"{"DistanceFadeRate":500,"EffectPriority":"ScreenEffectPriority_Lowest","EndTime":1,"bEnabled":true}"#,
    ),
    (
        "ParticleModuleSQEX_Gravity",
        r#"{"bApplyOwnerScale":true,"bEnabled":true}"#,
    ),
    (
        "ParticleModuleSQEX_If_Material_Scalar",
        r#"{"bEnabled":true}"#,
    ),
    (
        "ParticleModuleSQEX_InitSizeScale",
        r#"{"bIsScale":true,"bEnabled":true}"#,
    ),
    (
        "ParticleModuleSQEX_LocalParticle",
        r#"{"bIsLocalRotation":true,"bIsRotation":true,"bIsPosition":true,"bEnabled":true}"#,
    ),
    (
        "ParticleModuleSQEX_LocalTakeover",
        r#"{"bIsScale":true,"bIsRotation":true,"bIsPosition":true}"#,
    ),
    (
        "ParticleModuleSQEX_LocationBase",
        r#"{"SpawnRotCalcMode":"ESQEX_SPAWN_ROT_CALC_MODE_TANGENT","bApplySpawnLocationOwnerScale":true,"bApplyOwnerScale":true,"DirectionType":"EPARTICLE_DIR_TYPE_NONE","NormalDirection":0,"bProcessBurstList":true,"bProcessSpawnRate":true,"bEnabled":true}"#,
    ),
    ("ParticleModuleSQEX_Material", r#"{"bEnabled":true}"#),
    ("ParticleModuleSQEX_Material_Aura", r#"{"bEnabled":true}"#),
    (
        "ParticleModuleSQEX_Material_InLight",
        r#"{"ParamType":"SQEX_IN_LIGHT_TYPE_MAIN","bEnabled":true}"#,
    ),
    ("ParticleModuleSQEX_Material_Scalar", r#"{"bEnabled":true}"#),
    (
        "ParticleModuleSQEX_Material_SendParticlePosToMaterial",
        r#"{"bEnabled":true}"#,
    ),
    ("ParticleModuleSQEX_Material_Vector", r#"{"bEnabled":true}"#),
    ("ParticleModuleSQEX_MeshPivotOffset", r#"{"bEnabled":true}"#),
    (
        "ParticleModuleSQEX_OverrideParPostion",
        r#"{"bEnabled":true}"#,
    ),
    (
        "ParticleModuleSQEX_ParameterCollection",
        r#"{"bEnabled":true}"#,
    ),
    ("ParticleModuleSQEX_Parent", r#"{"bEnabled":true}"#),
    ("ParticleModuleSQEX_ParticleAround", r#"{"bEnabled":true}"#),
    ("ParticleModuleSQEX_ParticleColor", r#"{"bEnabled":true}"#),
    (
        "ParticleModuleSQEX_ParticleLocation",
        r#"{"bApplyOwnerScale":true,"EnableZ":true,"EnableY":true,"EnableX":true,"bEnabled":true}"#,
    ),
    (
        "ParticleModuleSQEX_ParticleRotation",
        r#"{"bEnabled":true}"#,
    ),
    (
        "ParticleModuleSQEX_ParticleRotationZ",
        r#"{"bEnabled":true}"#,
    ),
    (
        "ParticleModuleSQEX_ParticleScale",
        r#"{"bCurveParameterDirectScale":true,"bEnabled":true}"#,
    ),
    ("ParticleModuleSQEX_ParticleState", r#"{"bEnabled":true}"#),
    ("ParticleModuleSQEX_PlayerWearColor", r#"{"bEnabled":true}"#),
    ("ParticleModuleSQEX_PostprocessAura", r#"{"bEnabled":true}"#),
    (
        "ParticleModuleSQEX_PostprocessFringe",
        r#"{"EffectPriority":"ScreenEffectPriority_Lowest","bEnabled":true}"#,
    ),
    (
        "ParticleModuleSQEX_PostprocessGlare",
        r#"{"bEnabled":true}"#,
    ),
    (
        "ParticleModuleSQEX_Quadrangle",
        r#"{"SpawnRotCalcMode":"ESQEX_SPAWN_ROT_CALC_MODE_TANGENT","bApplySpawnLocationOwnerScale":true,"bApplyOwnerScale":true,"DirectionType":"EPARTICLE_DIR_TYPE_EMITTER_NORMAL","NormalDirection":1,"bProcessBurstList":true,"bProcessSpawnRate":true,"bEnabled":true}"#,
    ),
    (
        "ParticleModuleSQEX_Quadrangle_Seeded",
        r#"{"SpawnRotCalcMode":"ESQEX_SPAWN_ROT_CALC_MODE_TANGENT","bApplySpawnLocationOwnerScale":true,"bApplyOwnerScale":true,"DirectionType":"EPARTICLE_DIR_TYPE_EMITTER_NORMAL","NormalDirection":1,"bProcessBurstList":true,"bProcessSpawnRate":true,"bEnabled":true}"#,
    ),
    ("ParticleModuleSQEX_QuaternionTest", r#"{"bEnabled":true}"#),
    (
        "ParticleModuleSQEX_RadialBlur",
        r#"{"Mode":"SQEX_RADIAL_BLUR_MODE_RADIAL","DistanceFadeRate":500,"EffectPriority":"ScreenEffectPriority_Lowest","EndTime":1,"bEnabled":true}"#,
    ),
    (
        "ParticleModuleSQEX_RandomTestModule",
        r#"{"bEnabled":true}"#,
    ),
    (
        "ParticleModuleSQEX_Range",
        r#"{"SpawnRotCalcMode":"ESQEX_SPAWN_ROT_CALC_MODE_TANGENT","bApplySpawnLocationOwnerScale":true,"bApplyOwnerScale":true,"DirectionType":"EPARTICLE_DIR_TYPE_EMITTER_NORMAL","NormalDirection":1,"Direction":{"pitch":90,"yaw":0,"roll":0},"bProcessBurstList":true,"bProcessSpawnRate":true,"bEnabled":true}"#,
    ),
    (
        "ParticleModuleSQEX_Range_Seeded",
        r#"{"SpawnRotCalcMode":"ESQEX_SPAWN_ROT_CALC_MODE_TANGENT","bApplySpawnLocationOwnerScale":true,"bApplyOwnerScale":true,"DirectionType":"EPARTICLE_DIR_TYPE_EMITTER_NORMAL","NormalDirection":1,"Direction":{"pitch":90,"yaw":0,"roll":0},"bProcessBurstList":true,"bProcessSpawnRate":true,"bEnabled":true}"#,
    ),
    (
        "ParticleModuleSQEX_Raycast",
        r#"{"SpawnTheme":"ESQEX_RAYCAST_SPAWN_THEME_ONHIT","SpawnRotCalcMode":"ESQEX_SPAWN_ROT_CALC_MODE_TANGENT","bApplySpawnLocationOwnerScale":true,"bApplyOwnerScale":true,"DirectionType":"EPARTICLE_DIR_TYPE_NONE","NormalDirection":0,"bProcessBurstList":true,"bProcessSpawnRate":true,"bEnabled":true}"#,
    ),
    (
        "ParticleModuleSQEX_Ring",
        r#"{"VartexGenerateType":"EVARTEX_ODER_TYPE_ORDER","SpawnRotCalcMode":"ESQEX_SPAWN_ROT_CALC_MODE_TANGENT","bApplySpawnLocationOwnerScale":true,"bApplyOwnerScale":true,"DirectionType":"EPARTICLE_DIR_TYPE_EMITTER_NORMAL","NormalDirection":1,"bProcessBurstList":true,"bProcessSpawnRate":true,"bEnabled":true}"#,
    ),
    (
        "ParticleModuleSQEX_Ring_Seeded",
        r#"{"VartexGenerateType":"EVARTEX_ODER_TYPE_ORDER","SpawnRotCalcMode":"ESQEX_SPAWN_ROT_CALC_MODE_TANGENT","bApplySpawnLocationOwnerScale":true,"bApplyOwnerScale":true,"DirectionType":"EPARTICLE_DIR_TYPE_EMITTER_NORMAL","NormalDirection":1,"bProcessBurstList":true,"bProcessSpawnRate":true,"bEnabled":true}"#,
    ),
    (
        "ParticleModuleSQEX_ScreenBlur",
        r#"{"DistanceFadeRate":500,"EffectPriority":"ScreenEffectPriority_Lowest","EndTime":1,"bEnabled":true}"#,
    ),
    ("ParticleModuleSQEX_SpawnActor", r#"{"bEnabled":true}"#),
    (
        "ParticleModuleSQEX_SpawnFluidParticle",
        r#"{"SpawnRotCalcMode":"ESQEX_SPAWN_ROT_CALC_MODE_TANGENT","bApplySpawnLocationOwnerScale":true,"bApplyOwnerScale":true,"DirectionType":"EPARTICLE_DIR_TYPE_NONE","NormalDirection":0,"bProcessBurstList":true,"bProcessSpawnRate":true,"bEnabled":true}"#,
    ),
    (
        "ParticleModuleSQEX_SpawnParentHistoryPos",
        r#"{"SpawnRotCalcMode":"ESQEX_SPAWN_ROT_CALC_MODE_TANGENT","bApplySpawnLocationOwnerScale":true,"bApplyOwnerScale":true,"DirectionType":"EPARTICLE_DIR_TYPE_NONE","NormalDirection":0,"bProcessBurstList":true,"bProcessSpawnRate":true,"bEnabled":true}"#,
    ),
    (
        "ParticleModuleSQEX_SpawnParentHistoryPosEx",
        r#"{"SpawnRotCalcMode":"ESQEX_SPAWN_ROT_CALC_MODE_TANGENT","bApplySpawnLocationOwnerScale":true,"bApplyOwnerScale":true,"DirectionType":"EPARTICLE_DIR_TYPE_NONE","NormalDirection":0,"bProcessBurstList":true,"bProcessSpawnRate":true,"bEnabled":true}"#,
    ),
    (
        "ParticleModuleSQEX_SpawnParentParticlePos",
        r#"{"SpawnRotCalcMode":"ESQEX_SPAWN_ROT_CALC_MODE_TANGENT","bApplySpawnLocationOwnerScale":true,"bApplyOwnerScale":true,"DirectionType":"EPARTICLE_DIR_TYPE_NONE","NormalDirection":0,"bProcessBurstList":true,"bProcessSpawnRate":true,"bEnabled":true}"#,
    ),
    (
        "ParticleModuleSQEX_SpawnParticleSystem",
        r#"{"bGiveAlpha":true,"bGiveColor":true,"bGiveScale":true,"bGiveRotation":true,"bGiveLocation":true,"bEnabled":true}"#,
    ),
    (
        "ParticleModuleSQEX_SpawnSeaSurface",
        r#"{"SpawnRotCalcMode":"ESQEX_SPAWN_ROT_CALC_MODE_TANGENT","bApplySpawnLocationOwnerScale":true,"bApplyOwnerScale":true,"DirectionType":"EPARTICLE_DIR_TYPE_NONE","NormalDirection":0,"bProcessBurstList":true,"bProcessSpawnRate":true,"bEnabled":true}"#,
    ),
    (
        "ParticleModuleSQEX_SphereForce",
        r#"{"ForceData":{"HitParticleSizeScale":1,"HitMoveTime":0,"HitRotMovePow":0,"HitMovePow":0,"ForceRadius":50,"ForcePosition":{"x":0,"y":0,"z":0},"ParticleNoiseTypeOnHit":"SQEX_SPHERE_COLL_HIT_ON_NOISEL_TYPE_NONE","ForceType":"SQEX_PARTICLE_SPHERE_COLL_HIT_TYPE_SLIP"},"bEnabled":true}"#,
    ),
    ("ParticleModuleSQEX_TailNoise", r#"{"bEnabled":true}"#),
    (
        "ParticleModuleSQEX_TailShape",
        r#"{"TailScrollTimeScale":0,"bEnabled":true}"#,
    ),
    (
        "ParticleModuleSQEX_TailShapeExData_Inertia",
        r#"{"bEnabled":true}"#,
    ),
    (
        "ParticleModuleSQEX_TailShapeExData_Wave",
        r#"{"bEnabled":true}"#,
    ),
    ("ParticleModuleSQEX_TailShape_Base", r#"{"bEnabled":true}"#),
    (
        "ParticleModuleSQEX_TargetVelocity",
        r#"{"bApplyOwnerScale":true,"bEnabled":true}"#,
    ),
    ("ParticleModuleSQEX_TestModule", r#"{"bEnabled":true}"#),
    (
        "ParticleModuleSQEX_TexSpawn",
        r#"{"TexScaleV":1,"TexScaleU":1,"SpawnMaxSizeY":256,"SpawnMaxSizeX":256,"OnCenterTextrue":true,"SpawnRotCalcMode":"ESQEX_SPAWN_ROT_CALC_MODE_TANGENT","bApplySpawnLocationOwnerScale":true,"bApplyOwnerScale":true,"DirectionType":"EPARTICLE_DIR_TYPE_NONE","NormalDirection":0,"bProcessBurstList":true,"bProcessSpawnRate":true,"bEnabled":true}"#,
    ),
    (
        "ParticleModuleSQEX_ValuePerUnit",
        r#"{"MovementTolerance":0.1,"bEnabled":true}"#,
    ),
    ("ParticleModuleSQEX_VectorParameter", r#"{"bEnabled":true}"#),
    (
        "ParticleModuleSQEX_Vertex",
        r#"{"VartexGenerateType":"EVARTEX_ODER_TYPE_ORDER","SpawnRotCalcMode":"ESQEX_SPAWN_ROT_CALC_MODE_TANGENT","bApplySpawnLocationOwnerScale":true,"bApplyOwnerScale":true,"DirectionType":"EPARTICLE_DIR_TYPE_VERTEX_NORMAL","NormalDirection":1,"bProcessBurstList":true,"bProcessSpawnRate":true,"bEnabled":true}"#,
    ),
    (
        "ParticleModuleSQEX_VertexLinear",
        r#"{"VartexGenerateType":"EVARTEX_ODER_TYPE_ORDER","SpawnRotCalcMode":"ESQEX_SPAWN_ROT_CALC_MODE_TANGENT","bApplySpawnLocationOwnerScale":true,"bApplyOwnerScale":true,"DirectionType":"EPARTICLE_DIR_TYPE_NONE","NormalDirection":0,"bProcessBurstList":true,"bProcessSpawnRate":true,"bEnabled":true}"#,
    ),
    (
        "ParticleModuleSQEX_VertexSpawn",
        r#"{"VartexGenerateType":"EVARTEX_ODER_TYPE_ORDER","SpawnRotCalcMode":"ESQEX_SPAWN_ROT_CALC_MODE_TANGENT","bApplySpawnLocationOwnerScale":true,"bApplyOwnerScale":true,"DirectionType":"EPARTICLE_DIR_TYPE_VERTEX_NORMAL","NormalDirection":1,"bProcessBurstList":true,"bProcessSpawnRate":true,"bEnabled":true}"#,
    ),
    ("ParticleModuleSQEX_VertexTarget", r#"{"bEnabled":true}"#),
    (
        "ParticleModuleSQEX_VoxelVertexIndex",
        r#"{"bProcessBurstList":true,"bProcessSpawnRate":true,"bEnabled":true}"#,
    ),
    ("ParticleModuleSize", r#"{"bEnabled":true}"#),
    ("ParticleModuleSizeBase", r#"{"bEnabled":true}"#),
    (
        "ParticleModuleSizeMultiplyLife",
        r#"{"MultiplyZ":true,"MultiplyY":true,"MultiplyX":true,"bEnabled":true}"#,
    ),
    (
        "ParticleModuleSizeScale",
        r#"{"EnableZ":true,"EnableY":true,"EnableX":true,"bEnabled":true}"#,
    ),
    (
        "ParticleModuleSizeScaleBySpeed",
        r#"{"MaxScale":{"x":1,"y":1},"bEnabled":true}"#,
    ),
    ("ParticleModuleSize_Seeded", r#"{"bEnabled":true}"#),
    ("ParticleModuleSourceMovement", r#"{"bEnabled":true}"#),
    (
        "ParticleModuleSpawn",
        r#"{"bProcessBurstList":true,"bProcessSpawnRate":true,"bEnabled":true}"#,
    ),
    (
        "ParticleModuleSpawnBase",
        r#"{"bProcessBurstList":true,"bProcessSpawnRate":true,"bEnabled":true}"#,
    ),
    (
        "ParticleModuleSpawnPerUnit",
        r#"{"MovementTolerance":0.1,"UnitScalar":50,"bProcessBurstList":true,"bProcessSpawnRate":true,"bEnabled":true}"#,
    ),
    ("ParticleModuleSubUV", r#"{"bEnabled":true}"#),
    ("ParticleModuleSubUVBase", r#"{"bEnabled":true}"#),
    (
        "ParticleModuleSubUVMovie",
        r#"{"StartingFrame":1,"bEnabled":true}"#,
    ),
    ("ParticleModuleTrailBase", r#"{"bEnabled":true}"#),
    ("ParticleModuleTrailSource", r#"{"bEnabled":true}"#),
    ("ParticleModuleTypeDataAnimTrail", r#"{"bEnabled":true}"#),
    ("ParticleModuleTypeDataBase", r#"{"bEnabled":true}"#),
    (
        "ParticleModuleTypeDataBeam2",
        r#"{"Speed":10,"bEnabled":true}"#,
    ),
    (
        "ParticleModuleTypeDataMesh",
        r#"{"UVType":"SQEX_TAILPOL_UV_ROT_TYPE_NONE","MeshIndexIntervalTime":1,"SQEXBillboardType":"ESQEX_MESH_PAR_BILLBORAD_TYPE_NONE","bEnabled":true}"#,
    ),
    (
        "ParticleModuleTypeDataRibbon",
        r#"{"RenderAxis":"Trails_CameraUp","bEnabled":true}"#,
    ),
    ("ParticleModuleTypeDataSQEX_2DFluid", r#"{"bEnabled":true}"#),
    (
        "ParticleModuleTypeDataSQEX_CPUFluid",
        r#"{"bEnabled":true}"#,
    ),
    (
        "ParticleModuleTypeDataSQEX_ConnectParticle",
        r#"{"UVType":"SQEX_TAILPOL_UV_ROT_TYPE_NONE","MeshIndexIntervalTime":1,"SQEXBillboardType":"ESQEX_MESH_PAR_BILLBORAD_TYPE_NONE","bEnabled":true}"#,
    ),
    ("ParticleModuleTypeDataSQEX_Contin", r#"{"bEnabled":true}"#),
    ("ParticleModuleTypeDataSQEX_Control", r#"{"bEnabled":true}"#),
    (
        "ParticleModuleTypeDataSQEX_Decal",
        r#"{"ZFadePow":2,"ZFadeSegment":1,"bEnabled":true}"#,
    ),
    (
        "ParticleModuleTypeDataSQEX_DevilWave",
        r#"{"UVType":"SQEX_TAILPOL_UV_ROT_TYPE_NONE","MeshIndexIntervalTime":1,"SQEXBillboardType":"ESQEX_MESH_PAR_BILLBORAD_TYPE_NONE","bEnabled":true}"#,
    ),
    (
        "ParticleModuleTypeDataSQEX_DynamicShape",
        r#"{"UVType":"SQEX_PARTICLE_UV_ROT_TYPE_NONE","Size":10,"Radius":100,"bEnabled":true}"#,
    ),
    (
        "ParticleModuleTypeDataSQEX_FragmentMesh",
        r#"{"RandomImpactPower":500,"ImpactPower":100,"UVType":"SQEX_TAILPOL_UV_ROT_TYPE_NONE","SQEXBillboardType":"ESQEX_MESH_PAR_BILLBORAD_TYPE_NONE","bEnabled":true}"#,
    ),
    (
        "ParticleModuleTypeDataSQEX_GPUFluid",
        r#"{"bEnabled":true}"#,
    ),
    (
        "ParticleModuleTypeDataSQEX_GPUParticle",
        r#"{"bEnabled":true}"#,
    ),
    (
        "ParticleModuleTypeDataSQEX_GatherAfterBurst",
        r#"{"UVType":"SQEX_TAILPOL_UV_ROT_TYPE_NONE","MeshIndexIntervalTime":1,"SQEXBillboardType":"ESQEX_MESH_PAR_BILLBORAD_TYPE_NONE","bEnabled":true}"#,
    ),
    (
        "ParticleModuleTypeDataSQEX_GrassForce",
        r#"{"bEnabled":true}"#,
    ),
    (
        "ParticleModuleTypeDataSQEX_LineRoad",
        r#"{"bEnabled":true}"#,
    ),
    (
        "ParticleModuleTypeDataSQEX_OmnidLaser",
        r#"{"UVType":"SQEX_TAILPOL_UV_ROT_TYPE_NONE","MeshIndexIntervalTime":1,"SQEXBillboardType":"ESQEX_MESH_PAR_BILLBORAD_TYPE_NONE","bEnabled":true}"#,
    ),
    (
        "ParticleModuleTypeDataSQEX_PolygonMaker",
        r#"{"bEnabled":true}"#,
    ),
    (
        "ParticleModuleTypeDataSQEX_RestraintTail",
        r#"{"bEnabled":true}"#,
    ),
    (
        "ParticleModuleTypeDataSQEX_SkeletalMesh",
        r#"{"bEnabled":true}"#,
    ),
    (
        "ParticleModuleTypeDataSQEX_StaticMesh",
        r#"{"bEnabled":true}"#,
    ),
    (
        "ParticleModuleTypeDataSQEX_TailDecal",
        r#"{"ZFadePow":2,"ZFadeSegment":1,"UVType":"SQEX_TAILPOL_UV_ROT_TYPE_NONE","MeshIndexIntervalTime":1,"SQEXBillboardType":"ESQEX_MESH_PAR_BILLBORAD_TYPE_NONE","bEnabled":true}"#,
    ),
    (
        "ParticleModuleTypeDataSQEX_TailModelCirculation",
        r#"{"UVType":"SQEX_TAILPOL_UV_ROT_TYPE_NONE","MeshIndexIntervalTime":1,"SQEXBillboardType":"ESQEX_MESH_PAR_BILLBORAD_TYPE_NONE","bEnabled":true}"#,
    ),
    (
        "ParticleModuleTypeDataSQEX_TailModelMarch",
        r#"{"ShapeType":"ESQEX_SHAPE_TYPE_QUAD_AIR","UVType":"SQEX_TAILPOL_UV_ROT_TYPE_NONE","MeshIndexIntervalTime":1,"SQEXBillboardType":"ESQEX_MESH_PAR_BILLBORAD_TYPE_NONE","bEnabled":true}"#,
    ),
    (
        "ParticleModuleTypeDataSQEX_TailPolygon",
        r#"{"TextureVScale":1,"TextureUScale":1,"TailUVType":"SQEX_TAILPOL_UV_ROT_TYPE_NONE","RenderAxis":"Trails_CameraUp","bEnabled":true}"#,
    ),
    (
        "ParticleModuleTypeDataSQEX_TailShape",
        r#"{"bEnabled":true}"#,
    ),
    (
        "ParticleModuleTypeDataSQEX_TailShapeEx",
        r#"{"bEnabled":true}"#,
    ),
    (
        "ParticleModuleTypeDataSQEX_TailShapeTypeMesh",
        r#"{"UVType":"SQEX_TAILPOL_UV_ROT_TYPE_NONE","MeshIndexIntervalTime":1,"SQEXBillboardType":"ESQEX_MESH_PAR_BILLBORAD_TYPE_NONE","bEnabled":true}"#,
    ),
    (
        "ParticleModuleTypeDataSQEX_TailTube",
        r#"{"WNumber":3,"bEnabled":true}"#,
    ),
    (
        "ParticleModuleTypeDataSQEX_TestTypeData",
        r#"{"bEnabled":true}"#,
    ),
    (
        "ParticleModuleTypeDataSQEX_Thunder",
        r#"{"Speed":10,"bEnabled":true}"#,
    ),
    (
        "ParticleModuleTypeDataSQEX_TriangleParticle",
        r#"{"bEnabled":true}"#,
    ),
    (
        "ParticleModuleTypeDataSQEX_VoxelMesh",
        r#"{"bEnabled":true}"#,
    ),
    ("ParticleModuleVectorFieldBase", r#"{"bEnabled":true}"#),
    ("ParticleModuleVectorFieldGlobal", r#"{"bEnabled":true}"#),
    (
        "ParticleModuleVectorFieldLocal",
        r#"{"Intensity":1,"bEnabled":true}"#,
    ),
    ("ParticleModuleVectorFieldRotation", r#"{"bEnabled":true}"#),
    (
        "ParticleModuleVectorFieldRotationRate",
        r#"{"bEnabled":true}"#,
    ),
    ("ParticleModuleVectorFieldScale", r#"{"bEnabled":true}"#),
    (
        "ParticleModuleVectorFieldScaleOverLife",
        r#"{"bEnabled":true}"#,
    ),
    (
        "ParticleModuleVelocity",
        r#"{"bApplyOwnerScale":true,"bEnabled":true}"#,
    ),
    (
        "ParticleModuleVelocityBase",
        r#"{"bApplyOwnerScale":true,"bEnabled":true}"#,
    ),
    (
        "ParticleModuleVelocityCone",
        r#"{"bApplyOwnerScale":true,"bEnabled":true}"#,
    ),
    (
        "ParticleModuleVelocityInheritParent",
        r#"{"bApplyOwnerScale":true,"bEnabled":true}"#,
    ),
    (
        "ParticleModuleVelocityOverLifetime",
        r#"{"bApplyOwnerScale":true,"bEnabled":true}"#,
    ),
    (
        "ParticleModuleVelocity_Seeded",
        r#"{"bApplyOwnerScale":true,"bEnabled":true}"#,
    ),
    (
        "ParticleSpriteEmitter",
        r#"{"EmitterRenderMode":"ERM_Normal"}"#,
    ),
    (
        "ParticleSystem",
        r#"{"SQEXBaseColor":{"r":1,"g":1,"b":1,"a":1}}"#,
    ),
];
